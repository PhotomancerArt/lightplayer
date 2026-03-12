# Plan: Basic Const Support in GLSL Compiler

## Scope of Work

Implement basic `const` support in the lp-glsl compiler so that:
- Global const declarations like `const float PI = 3.14159;` compile and can be used in functions
- Local const (already partially works) is properly validated (read-only, must-init)
- Const qualifier semantics are enforced (spec variables.adoc §4.3.3, §4.3.3.1)

The main goal is to allow basic const support for simple things like PI. The plan considers broader context (constant folding, array sizes, error diagnostics) and phases work incrementally.

## Current State of Codebase

### What works
- **Local const inside functions**: `const float x = 1.0` in a function body is treated like a regular variable—declared, initialized, read. No write-check.
- **Const parameters**: `float f(const float x)` works (ParamQualifier::In); param qualifiers are parsed and used.
- **Filetest suite**: `filetests/const/` exists with qualifier/, expression/, builtin/, array-size/, scope/, errors/ subdirectories. Most tests use `[expect-fail]`.

### What does not work
- **Global const**: `ExternalDeclaration::Declaration` is skipped in FunctionExtractionPass (only processes FunctionDefinition). Global consts like PI are never in scope; references fail.
- **Array sizes with const**: `parse_array_dimensions` in type_resolver.rs only accepts `IntConst` literals. `float arr[ADD_SIZE]` with `const int ADD_SIZE = 5` would fail.
- **Const semantics**: No checks for must-init, read-only (write rejection), or constant initializer validation.
- **StorageClass**: VarDecl uses only `Local`; no `Const` storage class.

### Key files
- `lp-glsl-compiler/src/frontend/semantic/passes/function_extraction.rs` — only extracts FunctionDefinition
- `lp-glsl-compiler/src/frontend/semantic/scope.rs` — VarDecl, StorageClass (Local only)
- `lp-glsl-compiler/src/frontend/semantic/type_resolver.rs` — parse_array_dimensions requires IntConst
- `lp-glsl-compiler/src/frontend/codegen/stmt/declaration.rs` — emit_declaration
- `lp-glsl-compiler/src/frontend/glsl_compiler.rs` — compile flow, TypedShader has no global_constants

### glsl crate
- Uses `FullySpecifiedType` with qualifiers; `TypeQualifier` has `TypeQualifierSpec::Storage(StorageQualifier::Const)`
- Param qualifier extraction in `function_signature.rs` shows the pattern for reading qualifiers

## Questions

### Q1: Initial phase scope — literals only vs include constant expressions?
**Context**: Phase 1 can either (A) support only literal initializers (`const float PI = 3.14159`) or (B) include simple constant expressions (`const int N = 2 + 3`) from the start. (B) requires a minimal const-eval/folding pass.
**Answer**: (B) Literals + simple constant expressions from the start. Phase 1 includes minimal const-eval/folding.

### Q2: Where should constant folding live — frontend vs codegen?
**Context**: Constant expressions can be evaluated (1) during semantic analysis (frontend pass), (2) during codegen when emitting expressions, or (3) hybrid: frontend for init/array sizes, codegen inlines known values.
**Answer**: (C) Hybrid. Add `const_eval` module in frontend for initializers and array sizes; codegen inlines known const values when resolving variable references.

### Q3: StorageClass::Const vs is_const flag on VarDecl?
**Context**: We need to tag variables as const-capable. Option A: Add `StorageClass::Const`. Option B: Add `is_const: bool` to VarDecl.
**Answer**: (A) StorageClass::Const.

### Q4: Global const representation — TypedShader.global_constants or pre-populate symbol table?
**Context**: Global consts need to be available when compiling functions. Options: (A) Add `global_constants: HashMap<String, ConstValue>` to TypedShader; (B) Pre-populate each function's symbol table with global consts before validation/codegen; (C) Both—store in TypedShader and inject into per-function scope.
**Answer**: (C) Both. TypedShader holds the canonical map; validation and codegen pre-populate per-function scope from it.

### Q5: Array size constant expressions — same phase as global const or later?
**Context**: `const int N = 5; float arr[N];` requires resolving N when parsing array dimensions. This could be Phase 1 (if we add minimal const resolution) or Phase 2/3 (after global const + folding).
**Answer**: Phase 2 — global_constants and const-eval exist after Phase 1, so array size resolution is a straightforward next step.

### Q6: Error tests — update non-const-init.glsl expectation?
**Context**: `const/errors/non-const-init.glsl` currently expects "undefined variable" because const decls are ignored and BAD is never declared. When we implement const, we should reject the init line with "initializer must be constant expression" (or similar).
**Answer**: Yes. Update expected-error to correct message; may need new ErrorCode for const init. Include in error-handling phase.
