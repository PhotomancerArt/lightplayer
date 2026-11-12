# Const Test Coverage Analysis vs GLSL Spec

## Spec Reference (variables.adoc)

| Section | Topic | Key Requirements |
|---------|-------|------------------|
| §4.3.3 | Constant Qualifier | const = read-only; must be initialized at declaration |
| §4.3.3.1 | Constant Expressions | Literals, const refs, operators, constructors, builtin calls; user funcs disallowed |
| §4.3.3.1 | Constant Integral Expression | Subset for array sizes, case labels, etc. |
| §4.3.3 | Structure members | May not be qualified with const |
| §4.3.3 | Array size | Must be constant integral expression |

## Current Test Inventory

### Passing tests (21 files, ~60+ cases)

| Category | File | Spec § | Coverage |
|----------|------|--------|----------|
| **qualifier** | must-init.glsl | §4.3.3 | Global const with various types |
| **qualifier** | readonly.glsl | §4.3.3 | Read-only semantics via reads |
| **qualifier** | write-error.glsl | §4.3.3 | **Gap**: Only tests reads, not actual write rejection |
| **expression** | literal.glsl | §4.3.3.1 | Literal initializers |
| **expression** | reference.glsl | §4.3.3.1 | Const refs other const |
| **expression** | operators.glsl | §4.3.3.1 | +, -, *, /, % |
| **expression** | constructors.glsl | §4.3.3.1 | vec2/3, mat2 constructors |
| **expression** | unary.glsl | §4.3.3.1 | Unary minus |
| **builtin** | common.glsl | §4.3.3.1 | abs, min, clamp |
| **builtin** | trig.glsl | §4.3.3.1 | radians, sin |
| **builtin** | geometric.glsl | §4.3.3.1 | length, dot |
| **builtin** | exp.glsl | §4.3.3.1 | sqrt, pow |
| **array-size** | const-int.glsl | §4.3.3.1 | const int/uint as array size |
| **array-size** | const-expr.glsl | §4.3.3.1 | Expressions (2+3, 10-2, etc.) |
| **array-size** | local.glsl | §4.3.3.1 | Local const in array size |
| **array-size** | multidim.glsl | §4.3.3.1 | 2D/3D arrays with const dims |
| **array-size** | param.glsl | §4.3.3.1 | Const in function param array size |
| **scope** | global.glsl | §4.3.3 | Global const declaration |
| **scope** | local.glsl | §4.3.3 | Local const in function |
| **errors** | non-const-init.glsl | §4.3.3.1 | Non-const in init → error |
| **errors** | user-func.glsl | §4.3.3.1 | User func in init → error |

### Expect-fail (1 file)

| File | Reason |
|------|--------|
| struct-field.glsl | Structs not implemented; const in struct field sizes |

---

## Gaps and Proposed Additions

### 1. Error tests (high priority)

| Proposed test | Spec | Rationale |
|---------------|-------|-----------|
| **const-write-error.glsl** | §4.3.3 | `const float x = 1.0; x = 2.0;` → expected-error for write. Currently write-error.glsl only tests reads. |
| **const-no-init-error.glsl** | §4.3.3 | `const float x;` (no initializer) → expected-error must-init |
| **const-struct-member-error.glsl** | §4.3.3 | `struct S { const float x; };` → expected-error (structure members may not be qualified with const) |
| **const-array-index-non-const.glsl** | §4.3.3.1 | Array index with non-const in constant-expr context (e.g. unsized array indexed with variable) — if we support this path |

### 2. Expression coverage (medium priority)

| Proposed test | Spec | Rationale |
|---------------|-------|-----------|
| **const/builtin/missing-builtins.glsl** | §4.3.3.1 | Test remaining spec-listed builtins: trunc, round, ceil, mod, exp, log, exp2, log2, asin, acos, degrees. Currently only subset is tested. |
| **const/expression/array-access.glsl** | §4.3.3.1 | `const float arr[3] = float[3](1.0, 2.0, 3.0); const float x = arr[1];` — array element access in const init |
| **const/expression/swizzle.glsl** | §4.3.3.1 | `const vec3 v = vec3(1,2,3); const float x = v.x;` — component access in const init |

### 3. Edge cases / cleanup (lower priority)

| Item | Action |
|------|--------|
| **write-error.glsl** | Rename or add comment: "tests read path only; actual write rejection tested in const-write-error.glsl" |
| **non-const-init.glsl** | Verify expected-error message matches implementation (plan says "initializer must be constant expression" or similar) |
| **user-func.glsl** | Verify expected-error message matches (plan says "unknown constructor or non-const function") |
| **struct-field.glsl** | Keep expect-fail with SKIP comment; consider moving to a `const/skip/` or clearly document as "structs not implemented" |

### 4. Builtins not yet covered in const tests

Spec §4.3.3.1 lists these as const-foldable; const_eval has them; filetests missing:

- trunc, round, ceil (common)
- mod (common)
- exp, log, exp2, log2 (exponential)
- asin, acos (trig — we have sin, cos)
- degrees (trig — we have radians)

---

## Summary of Proposed New Tests

### High priority (error tests)

1. **const/errors/const-write-error.glsl** — Local const write: `const float x = 1.0; x = 2.0;` → expected-error. Requires codegen to reject writes to const (may need implementation).
2. **const/errors/const-no-init.glsl** — `const float x;` (no initializer) → expected-error. GlobalConstPass already rejects with "const `x` must be initialized"; add test.

### Medium priority (expand coverage)

3. **const/builtin/extended.glsl** — trunc, round, ceil, mod, exp, log, degrees, asin, acos (already in const_eval; add run tests).
4. **const/expression/array-access.glsl** — `const float arr[3] = float[3](1.0, 2.0, 3.0); const float x = arr[1];` — array element in const init. Check if const_eval supports array indexing.

### Lower priority

5. **const/errors/struct-member-const.glsl** — `struct S { const float x; };` — spec says structure members may not be qualified with const. Parser may reject; test if needed.

### Implementation note: const write rejection

- Global consts are inlined (not in variable_scopes), so `PI = 2.0` fails as "undefined variable" before we'd check write.
- Local consts ARE in variable_scopes; `const float x = 1.0; x = 2.0;` would currently succeed (no write check). Need to add is_const to VarInfo or check symbol table before write_lvalue.

---

## Cleanup

- **write-error.glsl**: Add comment: "Read path only; write rejection requires const-write-error.glsl + implementation."
- **struct-field.glsl**: Keep expect-fail; SKIP comment already documents structs not implemented.
- **non-const-init.glsl**, **user-func.glsl**: Verify expected-error messages match current compiler output.
