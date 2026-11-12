# Stage VI-C summary — fw-esp32 manifest + fw-emu gate

## Shipped

- **`fw-esp32/Cargo.toml`:** Removed orphan optional dependencies (`lps-cranelift`, `lps-jit-util`,
  `lps-builtins`, `cranelift-codegen`, `cranelift-frontend`, `cranelift-module`,
  `cranelift-control`, `target-lexicon`) and the unused optional **`lp-engine`** edge. Shader
  compilation remains **transitive**: `fw-esp32` → `lp-server` → `lp-engine` → `lpvm-cranelift`.
- **`lp-client`:** `[[test]]` entry for `scene_render_emu_async` with
  `required-features = ["serial"]` so default `cargo test -p lp-client` does not compile the
  emulator integration test without features.
- **`lp-engine`:** `let _ = ctx` in the `#[cfg(not(feature = "std"))]` `render` path to avoid
  unused-parameter warnings when building `fw-emu`.
- **`lpvm-cranelift`:** Split tests: `tests_options` always runs under `--no-default-features`; host
  **`jit_from_ir`** execution tests live in `#[cfg(all(test, feature = "std"))] mod tests` (without
  `std`, JIT targets RV32 — not executable on the host).

## A/B report

- [2026-03-26-lpvm-cranelift-vi-c-ab.md](../reports/2026-03-26-lpvm-cranelift-vi-c-ab.md) —
  methodology, fw-emu gate commands, sample alloc-trace count, `fw-esp32` ELF size, manual ESP32
  checklist (TBD).

## Handoff

- Re-run the **fw-emu gate** from the report before hardware sessions.
- Complete the **Manual ESP32** section in the A/B report after flashing.
- When an **old-compiler** worktree is available, re-run alloc-trace / compile-time rows for true
  old-vs-new deltas.

## Validation (recorded 2026-03-26)

`just build-fw-emu`, `just build-fw-esp32`, `cargo test -p fw-tests`, `cargo test -p lp-engine`,
`cargo test -p lp-server`, `cargo test -p lpvm-cranelift` (with/without default features, plus
`--features riscv32-emu`),
`cargo test -p lp-client --features serial --test scene_render_emu_async`,
`cargo clippy -p lp-engine -p lp-server -p lpvm-cranelift -p lp-client --all-features -- -D warnings`.
