# GLSL Frontend Memory Refactor Notes

## Scope

This plan refactors the `lps-glsl` frontend so on-device shader compilation has a deliberate memory shape on ESP32-C6.

The main target is the recursive typechecking path. Today it recursively returns and stores large owned HIR values. That makes each recursive call expensive on the stack and creates many small heap allocations while the compiler is already running in a tight embedded memory budget.

In scope:

- Measure and document the current frontend memory shape.
- Reduce the largest immediate stack frames in builtin/import typing helpers.
- Move typed expressions to arena-backed IDs so recursive typechecking returns small handles instead of large values.
- Move assignable places to arena-backed IDs and compact the most obvious place metadata.
- Update lowering to consume arena-backed HIR directly.
- Preserve full on-device GLSL JIT compilation for ESP32-C6.

Out of scope:

- Stubbing, feature-gating, or host-precompiling any part of the shader compiler.
- Replacing the compiler pipeline or changing GLSL language behavior.
- A full explicit-stack/non-recursive typechecker in the first pass.
- A custom bump allocator for all compiler temporary allocations in the first pass.
- Broad LPIR or native backend memory redesign.

## Current State

Relevant files:

- `lp-shader/lps-glsl/src/hir.rs`
- `lp-shader/lps-glsl/src/hir/types.rs`
- `lp-shader/lps-glsl/src/hir/typeck.rs`
- `lp-shader/lps-glsl/src/hir/builtin.rs`
- `lp-shader/lps-glsl/src/hir/builtin_out.rs`
- `lp-shader/lps-glsl/src/hir/coerce.rs`
- `lp-shader/lps-glsl/src/hir/const_fold.rs`
- `lp-shader/lps-glsl/src/hir/place.rs`
- `lp-shader/lps-glsl/src/lower.rs`
- `lp-shader/lps-glsl/src/lower/place/*`
- `lp-shader/lps-glsl/src/lower/ops/*`

Important current type shapes:

- `HirExpr` is a tree node with `span`, `ty`, and inline `HirExprKind`.
- `HirExprKind` stores children as `Box<HirExpr>` or `Vec<HirExpr>`.
- Call nodes store `Vec<HirExpr>`.
- `PlaceRead`, `Assign`, and `IncDec` store inline `HirAssignTarget`.
- `HirAssignTarget` stores inline `HirPlace`.
- `HirPlace` stores a root, `Vec<PlaceSegment>`, and final `LpsType`.
- `PlaceSegment::Index` stores `Box<HirExpr>`.
- `PlaceSegment::Swizzle` stores both `String` fields and `Vec<usize>` lanes.

Current measured sizes from the recent memory pass:

- `HirExpr`: 120 bytes on RV32.
- `HirExprKind`: 88 bytes.
- `HirPlace` / `HirAssignTarget`: 80 bytes.
- `LpsType`: 24 bytes.
- `ParsedExpr`: 32 bytes.

Current measured RV32 stack hotspots:

- `lps_glsl::hir::builtin::type_builtin_args`: about 2320 bytes.
- `TypeCtx::type_call`: about 2016 bytes.
- `TypeCtx::type_expr`: about 1360 bytes.
- `HirBuildJob::step`: about 1328 bytes.
- `TypeCtx::type_statements`: about 1264 bytes.
- `TypeCtx::type_binary_values`: about 1136 bytes.
- `TypeCtx::type_conditional`: about 1040 bytes.
- `lps_glsl::hir::builtin::type_glsl_import_args`: about 1008 bytes.
- `lower_hir`: about 784 bytes.
- `CompileJob::step`: about 672 bytes.
- `TypeCtx::type_place`: about 608 bytes.
- `TypeCtx::type_init_list`: about 592 bytes.

Relevant recent finding:

- A previous 4 KiB `driftsort` stack frame came from collecting function bodies into a `BTreeMap`. Incremental insertion removed that stack frame. That fixed one accidental hotspot but did not fix the underlying recursive-value HIR design.

Device context:

- Current known-good ESP32 heap reservation is `312_000` bytes in `lp-fw/fw-esp32/src/board/esp32c6/init.rs`.
- Increasing heap back toward `320_000` reduced available stack enough to expose stack corruption/faults.
- The latest known-good device trace after the previous cleanup was `traces/2026-05-19T16-43-37--esp32c6--demo-basic/trace.txt`.
- That trace showed project load after compile at about `152k free / 152k used`, with shader compile success around 192 ms.

## User Notes

- The on-device GLSL JIT compiler is the product. Do not make the compiler host-only or optional to fix memory pressure.
- The user is specifically worried that recursive typechecking has large stack frames.
- The user suspects some of this may be poor enum/data design in the new frontend.
- The user wants a memory sanity pass, not just isolated crash fixes.
- The user is worried about pulling heap back because the firmware is already heap-tight.
- The old compiler path worked with the old heap size, so the plan should look for introduced frontend memory growth rather than normalizing a larger stack reservation as the only fix.
- Device validation is now available through `just demo-esp32c6-check basic`.

## Open Questions

### Q1. Should the arena be only a typechecker scratch structure, or should lowering consume it directly?

Suggested answer: lowering should consume arena-backed HIR directly.

Context: a scratch-only arena would require freezing back into the old recursive `Box`/`Vec` tree before lowering. That may reduce typechecker stack but risks increasing peak heap because both the arena and rebuilt tree can exist during the compile. Direct lowering from the arena gives the cleaner embedded memory story.

### Q2. Should expressions and places move to IDs in the same phase?

Suggested answer: no. Move expressions first, then places.

Context: `ExprId` removes the biggest recursive return-value cost. `PlaceId` then shrinks assignment, read, writeback, and indexed-place nodes. Splitting the work keeps each review pass understandable.

### Q3. Should this plan implement a fully iterative typechecker?

Suggested answer: not initially.

Context: expression IDs make recursion much cheaper because the recursive calls return handles instead of full typed subtrees. An explicit worklist typechecker is still a useful future option, but it is more invasive around coercions, constant folding, lvalue typing, and out/inout writeback.

### Q4. Should this plan introduce a custom compiler bump allocator?

Suggested answer: not initially.

Context: arena-backed HIR already consolidates many tiny allocations into a few vectors and should reduce fragmentation pressure. A per-compile scratch allocator is attractive, but should follow after measurements show the remaining pressure.

### Q5. Should identifier/string interning be part of this pass?

Suggested answer: not in the main phases.

Context: strings are repeated in uniforms, globals, imports, texture paths, and place field segments. Interning could help, but it is a second-order improvement compared with large recursive HIR values and stack frames.

## Non-Goals

- Do not change shader semantics to make memory easier.
- Do not remove builtin coverage.
- Do not weaken tests, panic handling, or firmware check behavior.
- Do not replace recursive typechecking with an explicit worklist unless the arena refactor still leaves unsafe stack usage.
- Do not make `release-esp32` less representative of production just to pass memory tests.
