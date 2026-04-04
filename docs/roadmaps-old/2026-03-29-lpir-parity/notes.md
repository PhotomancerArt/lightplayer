# LPIR Parity Roadmap — Notes

## Scope

Bring the Naga → LPIR → (Cranelift | WASM) pipeline to feature parity with the old
`lp-glsl-cranelift` compiler for the **GLSL subset actually used by product shaders and filetests**.
Includes **arrays**; excludes **structs** (not used in production, low test count, high IR cost).

## Current state (2026-03-29, after initial parity plan work)

**Filetest pass rate (jit.q32):** 601 / 651 files pass (92%); 50 fail.

**Uncommitted work in tree:** ~888 insertions across 27 files (matrix metadata/invoke, relational
lowering, WASM emit, Q32 edge annotations, const error tests, expr_scalar fix). This is
substantial, not yet committed.

**50 failing files** break down by **root cause** (from running each file individually):

| Root cause | Files | Error signature | Example |
|-----------|-------|-----------------|---------|
| **`Relational` in `expr_type_inner`** | 15 | `expr_type_inner unsupported Relational { fun: All\|Any }` | `vec/bvec2/fn-all.glsl` |
| **Matrix `==`/`!=` (depends on Relational)** | 6 | same `Relational::All` through matrix comparison | `matrix/mat2/op-equal.glsl` |
| **Matrix element store** | 8 | `store to non-local pointer` | `matrix/mat2/incdec-matrix-element.glsl` |
| **Bvec `store to non-local pointer`** | 2 | same as matrix stores but on bvec index-assign | `vec/bvec2/assign-element.glsl` |
| **Bvec `Load from non-local pointer`** (dynamic index) | 5 | `Load from non-local pointer` | `vec/bvec2/access-array.glsl` |
| **Bvec cast / component mismatch** | 3 | `assignment component count 1 vs 2` | `vec/bvec2/to-float.glsl` |
| **`mix(bvec)` — Naga ambiguous overload** | 3 | `Ambiguous best function for 'mix'` | `vec/bvec2/fn-mix.glsl` |
| **`matrixCompMult` unknown function** | 1 | `Unknown function 'matrixCompMult'` (Naga parse) | `builtins/matrix-compmult.glsl` |
| **`isnan` / `isinf` — Naga `Float literal is infinite`** | 2 | parse error on `1.0/0.0` etc. | `builtins/common-isnan.glsl` |
| **Array types in LPIR** | 5 | `unsupported type for LPIR: Array { … }` | `array/declare-explicit.glsl` |
| **Forward-declare / param-unnamed with array/matrix poison** | 3 | file references array/matrix types in other declarations | `function/forward-declare.glsl` |
| **`while (bool j = expr)` — Naga parse** | 1 | `Expected LeftParen, found Identifier` | `control/while/variable-scope.glsl` |
| **`const` — single-failure edge** | 1 | 2/3 pass; 1 case (`round(2.5)` Q32 tie) | `const/builtin/extended.glsl` |
| **Struct types** | 2 | `unsupported type … Struct { … }` | `struct/define-simple.glsl` |

**Grouped by milestone-sized work:**

- **Relational / bvec expression type**: 15 + 6 matrix-eq = **21 files**. Root is `expr_type_inner` not handling `Relational`; the fix in `expr_scalar.rs` from phase 8 was partial (handles lowering but not the type-inference path used by some callers).
- **Pointer-based stores/loads (bvec dynamic index, matrix/bvec element assign)**: **15 files**. Non-local pointer patterns in lowering.
- **Bvec cast / mix / misc lowering**: **6 files**. Small, varied.
- **Array type lowering**: **5 files** unexpected-fail + many `@unimplemented`.
- **Naga frontend limitations** (parse errors, ambiguous overloads): **~4 files**. Upstream or fork fixes.
- **Polish / edge** (const tie, struct deferred, forward-declare poison): **~3 files**.

## Questions

### Q1 — Struct exclusion: mark remaining 2 struct files as @unimplemented or @unsupported?

**Context:** `struct/define-simple.glsl` and `struct/define-vector.glsl` are the only 2 struct
files that fail with unexpected errors. Many more struct tests are already `@unimplemented`. Structs
are explicitly out of scope.

**Suggested answer:** Mark these 2 files with `@unimplemented()` (we intend to implement structs
eventually, just not in this roadmap). This removes them from the "must fix" list.

**Answer:** Yes — mark with `@unimplemented()`, out of scope for this roadmap.

---

### Q2 — Naga parse-level failures: fix in fork or work around?

**Context:** 4 files fail due to **Naga's parser**:
- `builtins/common-isnan.glsl` / `common-isinf.glsl`: `Float literal is infinite` (Naga rejects
  `1.0/0.0` as a literal).
- `builtins/matrix-compmult.glsl`: `Unknown function 'matrixCompMult'` (Naga's GLSL frontend
  doesn't expose it under that name).
- `vec/bvec2/fn-mix.glsl` etc.: `Ambiguous best function for 'mix'` on `mix(vec, vec, bvec)`.
- `control/while/variable-scope.glsl`: `Expected LeftParen` for `while (bool j = expr)`.

Options:
1. Fix in Naga fork (we already maintain one).
2. Mark as `@unsupported` or `@unimplemented` with reason.
3. Rewrite the `.glsl` test to avoid the syntax Naga can't parse.

**Suggested answer:** Mixed: for `isnan`/`isinf` and `while(bool j=…)`, rewrite tests to avoid
the unparseable syntax (use a variable, not a literal expression). For `matrixCompMult` and
`mix(bvec)`, check if Naga supports an alternative spelling first; if not, mark
`@unimplemented(reason="Naga frontend limitation")` and optionally fix in the fork later.

**Answer:** Agreed — avoid modifying Naga. Rewrite tests where possible; mark
`@unimplemented(reason="Naga frontend limitation")` for the rest (`matrixCompMult`, `mix(bvec)`,
`while(bool j=…)`).

---

### Q3 — WASM parity: when to sweep?

**Context:** WASM and RV32 haven't been re-baselined since this work started. LPIR is shared so
most fixes carry over, but WASM-specific emit gaps may exist.

**Suggested answer:** Add a final "multi-backend sweep" milestone that runs `ALL_TARGETS` on the
full corpus and triages WASM/RV32-only failures. Do this **after** jit.q32 is at target bar, not
interleaved with each feature milestone.

**Answer:** Agreed — final milestone after jit.q32 is clean. Also build **multi-target comparison
tooling** into the filetest runner: a mode that runs all targets and generates a report showing
per-file/per-target status, cross-target discrepancies, and summary stats. This tooling is part of
the WASM/RV32 sweep milestone.

---

### Q4 — Done bar for this roadmap

**Context:** "All 651 files pass on jit.q32" was the original plan's bar, but that includes structs
(2 files) and Naga parse limitations (4 files) that are out of scope or require upstream work.

**Suggested answer:** "All jit.q32 files pass **except** those annotated `@unimplemented` (structs,
Naga limitations) or `@unsupported` (Q32 edge semantics)." Concretely: **0 unexpected failures on
`jit.q32`**; WASM/RV32 in a similar state modulo `@unimplemented(backend=wasm)` for known gaps.

**Answer:** Agreed. Done bar = `./scripts/glsl-filetests.sh` exits 0 on jit.q32 (0 unexpected
failures). Same for WASM/RV32 after the multi-backend milestone. Anything that can't pass is
annotated `@unimplemented` or `@unsupported` with a reason.

## Notes

_(Iteration notes from user answers.)_
