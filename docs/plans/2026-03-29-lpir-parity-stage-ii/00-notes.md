# Plan notes: LPIR parity — stage II (Milestone II)

Roadmap: [`docs/roadmaps/2026-03-29-lpir-parity/milestone-ii-pointer-stores-loads.md`](../../roadmaps/2026-03-29-lpir-parity/milestone-ii-pointer-stores-loads.md).

## Scope of work

Per the roadmap (refined against the repo today):

- Eliminate **`store to non-local pointer`** / **`Load from non-local pointer`** for the patterns Naga actually emits for:
  - **Matrix element compound updates** (`++m[c][r]`, `m[c][r]++`, decrements) — filetests e.g. `matrix/mat{2,3,4}/incdec-matrix-element.glsl`.
  - **Matrix column inc/dec** — `incdec-matrix-column.glsl` (may interact with existing “store to matrix column not supported” branch).
  - **Bvec indexed assign** — `a[i] = …` where `i` is a **literal** subscript; `.x` / `.y` assign already works (`vec/bvec*/assign-element.glsl`).
  - **Bvec (and consistent vec) dynamic index load** — `a[i]` with variable `i`; today `lower_expr.rs` rejects `Expression::Access` with *dynamic vector access not supported* (`vec/bvec*/index-variable-*.glsl`, `access-array.glsl`).

Explicitly **out of scope** (per roadmap): full **array** addressing (Milestone IV), struct members, matrix invoke/ABI (Milestone V).

## Current state of the codebase

- **`lower_stmt.rs` — `Statement::Store`**
  - Handles `Store(AccessIndex(LocalVariable))` for **vector** component stores (float/int path).
  - Handles `Store(AccessIndex(AccessIndex(LocalVariable)))` for **matrix element** `m[col][row] = scalar` with flat index `col * rows + row`.
  - Returns *store to matrix column not supported* for `Store(AccessIndex(LocalVariable))` when the local is a **matrix** (column assign).
  - Anything else under `AccessIndex` falls through to **`store to non-local pointer`**.

- **Empirical JIT failures (2026-03-29)**
  - `matrix/mat2/incdec-matrix-element.glsl`: `++m[0][0]` → **`unsupported statement: store to non-local pointer`** (all similar tests in file, not only the one marked `@unimplemented`).
  - `vec/bvec2/assign-element.glsl`: `a.x = true` **passes**; `a[0] = true` → **`store to non-local pointer`** (literal index). Suggests Naga uses **`Expression::Access`** for `[]` and **`AccessIndex`** for named components, and stores target the `Access` pointer shape.

- **`lower_expr.rs`**
  - `Expression::Access { .. }` → error **`dynamic vector access not supported`** (loads and any value use).
  - `Expression::Load { pointer }` only peels `LocalVariable`, `AccessIndex` (recursive), and `FunctionArgument` pointers; other bases → **`Load from non-local pointer`**.

- **Milestone roadmap vs code**
  - The roadmap text that matrix stores are unimplemented is **stale** for the simple `m[i][j] = x` nested-`AccessIndex` case; the **remaining** work is aligning **statement and expression lowering** with Naga’s **`Access`**-based subscripts and compound-update store shapes.

## Questions (to resolve with the user)

Each question will be asked one at a time in chat; answers are recorded below under **# Answers** and in **# Notes** as needed.

### Q1 — Filetest success criteria (backend / float mode)

**Question:** Stage I required **`jit.q32`**, **`wasm.q32`**, and **`rv32.q32`** green on the Tier A slice. Milestone II roadmap text says *“pass on `jit.q32`”* only. Should stage II treat **all three targets** as mandatory for the Milestone II corpus, or ship **JIT-first** and defer wasm/rv32 triage (e.g. traps or backend-specific quirks) to a follow-up?

**Suggested default:** Match stage I: define an **`expected-passing-tests.md`** list for stage II and require **all three** targets to pass (with `@unsupported` only where normatively justified, same policy as q32.md / filetest annotations).

### Q2 — Matrix column inc/dec vs element-only

**Question:** `incdec-matrix-column.glsl` and whole-column assign touch **column** pointers (`m[i]++` style). The roadmap lists “matrix column inc/dec” as in scope, but `lower_stmt` currently rejects **column** stores on matrix locals. Should stage II **include** lowering for column-level read-modify-write (and whole-column assign if Naga emits it), or **narrow** scope to **scalar element** inc/dec only and leave column tests marked unimplemented?

**Suggested default:** **Include** column inc/dec: extend store/load paths for the Naga shapes that correspond to a **vector column** (same scalarized vreg layout as column `AccessIndex` reads).

### Q3 — `Expression::Access` lowering strategy

**Question:** For dynamic index `v[i]` / `b[i]`, choose implementation approach:

- **A)** Lower `Access` to a **select chain** on the index for `bvec2`–`bvec4` / `vec2`–`vec4` / `ivec2`–`ivec4` (roadmap suggests this for bvec; same idea extends to int/float vectors).
- **B)** Spill vector to **stack / locals** and use **byte offset + Load/Store** (more general, heavier).
- **C)** **A for small fixed sizes**, **B** only if needed for a case Naga emits that select cannot represent.

**Suggested default:** **C** — select chains for 2–4 components (matches roadmap WASM alignment intent); add stack path only if a test requires it.

---

## Answers

### Q1 — Filetest success criteria

**Decision:** Match stage I — define an explicit stage-II corpus (e.g. `expected-passing-tests.md`) and require **`jit.q32`**, **`wasm.q32`**, and **`rv32.q32`** to pass; use `@unsupported` only where normatively justified (same policy as stage I / `q32.md`).

### Q2 — Matrix column inc/dec vs element-only

**Decision:** **Include** column-level inc/dec / column vector stores and loads in stage II — extend `lower_stmt` / `lower_expr` for Naga shapes that target a matrix **column as a vector**, consistent with existing column-major scalarized vregs.

### Q3 — `Expression::Access` lowering strategy

**Decision:** **Option C** — lower **`Access`** on `vec2`–`vec4` / `ivec2`–`ivec4` / `bvec2`–`bvec4` with **icmp + select chains** (same spirit as the old compiler’s read path). Add **stack / offset Load/Store** only if a Naga shape or test **cannot** be expressed with selects. Introduce **shared helpers** (e.g. `lower_access_*`) so matrix `Access` (column / element) can reuse logic.

## Notes

### Legacy `lp-glsl-compiler` (`feature/lightplayer-main`) behavior

- **Reads** (`translate_matrix_indexing` in `lp-glsl-compiler/.../expr/component.rs`): variable index on a **vector** uses **`emit_bounds_check`** (trapnz) then a **select chain** over components; variable **matrix column** uses bounds check then **per-row** selects across columns.
- **Writes (LValue bracket indexing)** (`resolve_matrix_vector_indexing` in `lvalue/resolve/indexing/matrix_vector.rs`): indices are passed through **`validate_index`**, which requires a **compile-time constant** (`IntConst`, etc.). So **`v[i] = …` with runtime `i`** was **not** supported on the **assignment** path in that compiler — only **constant** `v[0] = …` style for matrix/vector LValues.
- **Stage II in `lp-glsl-naga`:** we **do** need to support whatever **Naga emits** (including `Expression::Access` for `a[0] =` and compound updates), which goes **beyond** the old compiler’s constant-only LValue indexing.

**User confirmation (2026-03-29):** Proceed with **option C**; accept that we must implement Access-based stores/loads even though the old compiler did not fully cover writes with variable indices.
