# Phase 7 — Cleanup & validation

## Scope of phase

- Grep the working tree for **`TODO`**, **`FIXME`**, stray **`dbg!`**,
  debug **`println!`** introduced during this plan.
- Fix warnings: unused imports left over from scaffold phases,
  **`dead_code`** on test-only helpers (prefer **`#[allow(dead_code)]`**
  with a one-line reason or remove).
- Re-skim the public surface in `lpir/src/lib.rs` — only
  **`inline_module`** and **`InlineResult`** should be exported from
  the inliner; everything else stays crate-private.
- Confirm `log::debug!` calls are at the right level (decisions =
  debug; per-op chatter, if any was added during bring-up, must be
  removed or downgraded to `trace`).
- Run the **full validation matrix** below.

## Cleanup & validation

```bash
# Per-crate tests.
cargo test -p lpir
cargo test -p lpvm-native
cargo test -p lpvm-cranelift
cargo test -p lpvm-wasm

# Filetests (M2.5 backend no-op arms must not regress anything).
cargo test -p lps-filetests -- --test-threads=4

# Embedded build path — required by no-std-compile-path rule.
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
  --profile release-esp32 --features esp32c6,server

# Other consumers of lpir if applicable to this workspace's AGENTS list
# (e.g. fw-emu, lp-server). Add as required.
cargo check -p fw-emu
cargo check -p lp-server
```

Expected results:

- All existing tests pass — the inliner is opt-in and not yet wired
  into `compile_module` (M4 wires it).
- M2.5 marker round-trips through parse/print and validate stays
  silent on legacy loops (no `Continuing` marker) and on new loops
  (with `Continuing` marker).
- No new warnings under `-D warnings` if the workspace enforces it.

## Plan cleanup

- Write **`docs/plans/2026-04-17-lpir-inliner-stage-iii/summary.md`**:
  bullets — what shipped (`LpirOp::Continuing` marker, `inline` module,
  `inline_module` public API, callgraph + topo order, per-param scan +
  alias-or-copy remap, body splicer, heuristic with `func_weight =
  body.len()`, structural `recompute_offsets`), crates touched
  (`lpir`, `lpvm-native`, `lpvm-cranelift`, `lpvm-wasm`), follow-ups
  (M3.1 empirical `func_weight` tuning, M4 wire into
  `compile_module` + GLSL filetests with `compile-opt`, M5 dead-func
  elimination, future-work removal of denormalized offset fields).
- Move **`docs/plans/2026-04-17-lpir-inliner-stage-iii/`** →
  **`docs/plans-done/2026-04-17-lpir-inliner-stage-iii/`** when
  implementation is complete.

## Commit (when requested)

Single Conventional Commits message covering both M2.5 and M3:

```
feat(lpir): inliner pass + Continuing marker op (M3 + M2.5)

- Add LpirOp::Continuing structural marker for loop continuing block;
  cached LoopStart::continuing_offset retained for backend efficiency.
  No-op arms in lpvm-native, lpvm-cranelift, lpvm-wasm.
- Add lpir::inline module: inline_module public API, call graph with
  bottom-up topological order and cycle skipping, per-param
  scan-then-alias-or-copy remap, body splicer with return-shape
  analysis, structural recompute_offsets, heuristic gated by
  InlineConfig (mode + thresholds + budgets).
- Decisions emitted at log::debug for CLI observability.
- Empirical func_weight tuning deferred to M3.1; dead-func elimination
  deferred to M5; pipeline wiring + GLSL filetests deferred to M4.
```

## Code Organization Reminders

- Final pass: no temporary hacks without **`TODO(plan):`** if something
  must remain. Any remaining TODOs must reference a follow-up
  milestone (M3.1 / M4 / M5 / future-work).
- Keep the inliner crate-private surface tight — future contributors
  should be able to refactor `inline/` internals without touching
  any other crate.
