# M5 read-only `in` aggregate — measurement notes

**Date:** 2026-04-23  
**Scope:** Phase 2 of the read-only `in` optimisation (frontend elision of stack slot + prologue `Memcpy` when the scan classifies a by-value aggregate parameter as read-only).

## Methodology

- **Correctness:** `cargo clippy -p lps-frontend -- -D warnings` and `scripts/filetests.sh --concise` from the repo root (no filetest expectation changes for M5).
- **Cycle / instruction deltas:** No dedicated micro-benchmark or stable “before” snapshot is checked in for this change. The optimisation is **semantics-preserving** relative to the memcpy path: read-only parameters still use the same parameter pointer as the old copy source; the expected win is **omitted** `alloc_slot`, `SlotAddr` for the copy destination, and the prologue **`Memcpy`** into that slot.

## Commands attempted

- **Shader instruction / size comparison (optional):**  
  `scripts/shader-debug.sh -t rv32n --summary <file.glsl>`  
  (`cargo run -p lp-cli -- shader-debug`, see `scripts/shader-debug.sh`).  
  Useful for eyeballing instruction counts on specific entry points; not run as a gated before/after for M5 because no single committed shader isolates read-only `in` aggregates with a fixed harness.

- **Rainbow / examples:** Paths such as `examples/basic/src/rainbow.shader/main.glsl` are available for manual `shader-debug` runs; they do not specifically target aggregate-parameter memcpy, so they are not used as regression numbers for M5.

## Expected effect (when read-only applies)

- Fewer LPIR ops in the callee prologue (no memcpy from the incoming pointer into a fresh stack slot for that parameter’s proxy local).
- Downstream backends see fewer memory operations on the hot path for read-only aggregate `in` arguments.

## Targets

- **rv32n / rv32c / wasm / emu:** Behaviour is defined at the LPIR level; any cycle delta depends on the chosen VM and codegen. Use `shader-debug.sh -t …` with the backend you care about when comparing two compiler revisions locally.
