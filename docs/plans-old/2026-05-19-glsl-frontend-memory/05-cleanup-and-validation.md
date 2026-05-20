# Phase 5: Cleanup And Final Validation

## Scope Of Phase

Remove migration residue, tighten the final memory story, and run the complete validation set.

In scope:

- Remove temporary compatibility helpers from arena migration.
- Remove stale comments, dead code, and TODOs introduced by the refactor.
- Confirm the lowerer consumes arena-backed HIR directly.
- Update `measurements.md` with before/after results.
- Run the final shader-pipeline validation commands.
- Run hardware validation with `just demo-esp32c6-check basic` when the ESP32-C6 is attached.

Out of scope:

- New architectural work not required to finish the arena refactor.
- Adding a custom bump allocator.
- Implementing explicit-stack typechecking.
- Broad string interning.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any remaining temporary code with a clear `TODO`, but prefer removing it.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Review:

- `lp-shader/lps-glsl/src/hir/arena.rs`
- `lp-shader/lps-glsl/src/hir/types.rs`
- `lp-shader/lps-glsl/src/hir/typeck.rs`
- `lp-shader/lps-glsl/src/hir/builtin.rs`
- `lp-shader/lps-glsl/src/hir/coerce.rs`
- `lp-shader/lps-glsl/src/hir/place.rs`
- `lp-shader/lps-glsl/src/lower.rs`
- `lp-shader/lps-glsl/src/lower/place/*`
- `lp-shader/lps-glsl/src/lower/ops/*`

Remove:

- Any permanent freeze-from-arena-to-tree helper.
- Temporary `Vec<ExprId>` compatibility storage if `ExprList` is implemented.
- Stale references to expression children being boxed.
- Stale references to place index expressions being boxed.
- Debug-only size print code.
- Commented-out experiments.

Confirm:

- `TypeCtx::type_expr` returns `ExprId`.
- Hot coercion helpers do not return `(HirExpr, HirExpr, LpsType)` by value.
- `HirStmt` stores expression IDs, not owned expression trees.
- `HirFunctionBody` owns the arena needed by its statements.
- `lower_expr` receives an arena and `ExprId`.
- Place writebacks use `PlaceId`.
- Firmware heap reservation remains at the known-good value unless the user explicitly approved otherwise.

Update:

- `docs/plans/2026-05-19-glsl-frontend-memory/measurements.md`

The final measurements should include:

- Baseline summary.
- Final type sizes or equivalent arena node sizes.
- Final RV32 stack hotspot summary.
- Final device trace path.
- Final device memory lines.
- Any remaining risk or recommended follow-up.

## Validate

```bash
cargo fmt --check
cargo test -p lps-glsl
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p lpa-server
cargo test -p lpa-server --no-run
just demo-esp32c6-check basic
```

If hardware is unavailable, report that explicitly and leave the plan incomplete from a device-confidence standpoint.
