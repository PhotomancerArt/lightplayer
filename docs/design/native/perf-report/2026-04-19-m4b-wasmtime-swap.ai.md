# M4b — Host backend swap (Cranelift → Wasmtime) — perf snapshot

Plan: `docs/plans-old/2026-04-19-m4b-host-backend-swap/`
Date: 2026-04-19

## Summary

Swap of `lp-engine`'s host shader backend from `lpvm-cranelift` to
`lpvm-wasm` (wasmtime). Backend selection moved from Cargo features
to `cfg(target_arch = …)`. See the plan dir for full context.

## Measurements

| Metric                             | Pre-swap | Post-swap     | Delta |
|------------------------------------|----------|---------------|-------|
| `time cargo check -p lp-server` (warm, 2nd run) | —        | 0.22 s        | —     |
| `time cargo check -p lp-server` (cold, 1st run) | —        | 21.43 s       | —     |
| `lp-cli` release binary size       | —        | 17,431,968 B (~16.6 MiB) | —     |
| Cold cargo check (clean target)    | —        | not captured  | —     |

No pre-M4b baseline was captured — the plan commits as one unit and
no pre-M4b worktree was set up. Numbers are recorded for future
reference. The cold `cargo check -p lp-server` number above
includes wasmtime + cranelift compilation (cranelift is still in the
workspace for `lp-cli shader-debug` AOT and lpfx-cpu).

## Multi-shader stress

Five parallel runs of `cargo test -p lp-engine --test scene_render
--test scene_update --test partial_state_updates -- --test-threads=8`
(see plan phase 4, step 2). The Cranelift JIT-state-leakage failure
pattern (`"function must be compiled before it can be finalized"`)
did **not** reproduce — across five fully parallel runs, every
binary reported `test result: ok. 3 passed; 0 failed; 0 ignored`,
and `grep -E '(FAILED|panicked|finalized)'` over all five logs
matched zero lines.

Result: **pass.** No JIT-state leakage on the new wasmtime host
path, which is the named M4b motivator.

## Notes

- Wasmtime defaults left unchanged beyond `consume_fuel(true)`
  and a 64 MiB pre-grown linear memory budget
  (`WasmOptions::host_memory_pages = 1024`). Deferred knobs:
  epoch interruption, parallel compilation, custom memory
  reservation. See `lpvm-wasm/src/rt_wasmtime/engine.rs`.
- `lpvm-cranelift` stays in the workspace for `lp-cli shader-debug`
  AOT and (until M4c) `lpfx-cpu`. `lp-engine` no longer depends on
  it.
- Backend selection is now target-arch driven (RV32 →
  `lpvm-native`, wasm32 → `lpvm-wasm` browser, catchall →
  `lpvm-wasm` wasmtime). No backend Cargo feature on `lp-engine`
  or `lp-server`.
- Pre-existing intermittent flake observed during validation: the
  `lpvm-cranelift --lib` test binary occasionally fails when run
  with the default parallel test harness (e.g. `tests::test_call_in_loop`
  asserting `26707105 == 5`, or `signal: 5, SIGTRAP`). Passes
  reliably with `--test-threads=1`. This is the JIT-state-leakage
  flake that motivated M4b on the *engine* side; `lpvm-cranelift`
  itself was not changed by this plan, so the flake remains in
  that crate's own test suite. Out of scope here. `cargo test
  --workspace` should be run with `--no-fail-fast` until
  `lpvm-cranelift` is either retired (post-M4c) or its test
  parallelism issue is fixed independently.
- Pre-existing `unexpected_cfgs` warnings on `host_debug!` in
  `fw-emu/src/main.rs` (target=riscv32) — unrelated to this plan,
  leave alone.
- Phase 4 ran `cargo fmt --all` once to fix formatting drift
  introduced by phases 1–3 (e.g., `gfx/mod.rs` mod-decl ordering,
  line wrapping in `rt_wasmtime/{engine,shared_runtime}.rs`,
  import ordering in `lpvm-wasm/tests/compile_with_config.rs`).
  That was the minimum intervention needed for `just check` /
  `just ci` to pass; no behavioural code changed.
