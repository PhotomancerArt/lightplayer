# M7 — Control flow cleanup (plan / status)

**Re-run:** 2026-04-24 on current tree (`wasm.q32`, `rv32c.q32`, `rv32n.q32` only; `jit.q32` out of scope).

## Section F checklist (`docs/reports/2026-04-23-filetest-triage/broken.md`)

| Item | File / case | wasm.q32 | rv32c.q32 | rv32n.q32 | Action |
|------|-------------|----------|-----------|-----------|--------|
| F1 | `control/ternary/types.glsl` — `test_ternary_struct_complex` | ~~Fail~~ **pass** | ~~stale marker~~ **pass** | ~~Fail~~ **pass** | **Done:** `c2` uses `0.5` triple so `(r+g+b)*10` is exact in q32; removed `@broken` |
| F2 | `control/edge_cases/loop-expression-scope.glsl` — `test_for_loop_expression_modified_in_body` | ~~Fail~~ **pass** | **pass** | **pass** | **Done:** expectation `== 6` (body then loop-expr); removed `@broken` |

## Out of scope (this milestone)

- Broad CFG / optimizer refactors.
- `jit.q32` annotations left as-is unless they block product targets (they do not).

## Done (fill when closed)

- [x] F1 resolved on all three targets (2026-04-24)
- [x] F2 resolved on all three targets (2026-04-24)
- [x] Targeted filetests: 54/54 pass on `wasm.q32`, `rv32c.q32`, `rv32n.q32`
- [x] `cargo check -p lps-filetests-app` OK
- [ ] `cargo clippy -p lps-filetests-app -- -D warnings` — fails in dependency `lpvm-wasm` (`dead_code` on `emit_q32_ftoi_sat_*`); pre-existing, not introduced by M7 file edits

## Blockers (none if empty)

- **Clippy:** clean `lpvm-wasm` dead_code (or allow) if CI runs clippy with `-D warnings` on this graph.
