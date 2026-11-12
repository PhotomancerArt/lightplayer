# GLSL Frontend Memory Measurements

## Baseline

- **Date:** 2026-05-19
- **Branch:** `feature/radio`
- **Commit:** `a62ca46a`
- **ESP32-C6 heap reservation:** `312_000` bytes in `lp-fw/fw-esp32/src/board/esp32c6/init.rs`

## Baseline Type Sizes

Inherited from the previous memory pass:

| Type | RV32 size |
| --- | ---: |
| `HirExpr` | 120 bytes |
| `HirExprKind` | 88 bytes |
| `HirPlace` | 80 bytes |
| `HirAssignTarget` | 80 bytes |
| `LpsType` | 24 bytes |
| `ParsedExpr` | 32 bytes |

## Baseline RV32 Stack Hotspots

Inherited from the previous memory pass:

| Symbol | Stack |
| --- | ---: |
| `lps_glsl::hir::builtin::type_builtin_args` | 2320 bytes |
| `TypeCtx::type_call` | 2016 bytes |
| `TypeCtx::type_expr` | 1360 bytes |
| `HirBuildJob::step` | 1328 bytes |
| `TypeCtx::type_statements` | 1264 bytes |
| `TypeCtx::type_binary_values` | 1136 bytes |
| `TypeCtx::type_conditional` | 1040 bytes |
| `lps_glsl::hir::builtin::type_glsl_import_args` | 1008 bytes |
| `lower_hir` | 784 bytes |
| `CompileJob::step` | 672 bytes |
| `TypeCtx::type_place` | 608 bytes |
| `TypeCtx::type_init_list` | 592 bytes |

## Baseline Device Trace

Latest known-good trace before this implementation:

- `traces/2026-05-19T16-43-37--esp32c6--demo-basic/trace.txt`

Important lines:

```text
[INFO] lpa_server::handlers: [mem] stop_all_projects before: 233k free / 71k used
[INFO] lpa_server::handlers: [mem] stop_all_projects after: 233k free / 71k used
[INFO] lpa_server::handlers: [mem] load_project before: 228k free / 76k used
[INFO] lpa_server::handlers: [mem] load_project after: 152k free / 152k used
[INFO] lpc_engine::nodes::shader::shader_node: [shader-node] compilation succeeded (node=NodeId(4), elapsed=192ms, lpir_inst_count=578, lpir_func_count=12, lpir_import_count=7, final_inst_count=1896, final_code_size=7584 bytes)
```

## Acceptance Criteria

- Do not increase the ESP32-C6 heap reservation as the primary fix.
- Stop passing/returning `HirExpr` recursively by value.
- Reduce the largest `lps-glsl` typechecker stack frames materially after the arena phases.
- Keep full on-device GLSL JIT compilation enabled for ESP32-C6.
- `just demo-esp32c6-check basic` should complete without panic/OOM on attached hardware.

## Final Implementation Notes

- `TypeCtx::type_expr` now returns `ExprId` instead of `HirExpr`.
- `HirFunctionBody` owns a function-local `HirArena`.
- `HirStmt` stores `ExprId` handles instead of owned expression trees.
- `HirExprKind` children are `ExprId` or `ExprList` instead of `Box<HirExpr>` / `Vec<HirExpr>`.
- Assign/read/inc-dec expression nodes store `PlaceId`.
- `HirUserCallWriteback` stores `PlaceId`.
- `PlaceSegment::Index` stores `ExprId` instead of `Box<HirExpr>`.
- Uniform/global expression nodes no longer carry unused `String` names.
- Lowering consumes the arena directly; there is no freeze back to the old recursive HIR tree.

## Final Device Trace

Device validation after the arena refactor:

- `traces/2026-05-19T17-09-04--esp32c6--demo-basic/trace.txt`
- `traces/2026-05-19T17-09-04--esp32c6--demo-basic/report.txt`

Important lines:

```text
[INFO] lpa_server::handlers: [mem] stop_all_projects before: 233k free / 71k used
[INFO] lpa_server::handlers: [mem] stop_all_projects after: 233k free / 71k used
[INFO] lpa_server::handlers: [mem] load_project before: 228k free / 76k used
[INFO] lpa_server::handlers: [mem] load_project after: 152k free / 152k used
[INFO] lpc_engine::nodes::shader::shader_node: [shader-node] compilation succeeded (node=NodeId(4), elapsed=189ms, lpir_inst_count=578, lpir_func_count=12, lpir_import_count=7, final_inst_count=1896, final_code_size=7584 bytes)
```

The ESP32-C6 heap reservation remained `312_000` bytes.

## Final Validation

Passed:

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

Also passed:

```bash
RUSTFLAGS='-Z emit-stack-sizes' cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

No machine-readable stack-size report was emitted in an easy-to-consume sidecar file by this local toolchain run, so this document records the structural stack fix and device result rather than a guessed final byte table.
