# Plan: Const Globals Support

## Scope of Work

Add support for **const globals** in the GLSL compiler. Const globals are compile-time constants that should be **substituted at compile time** rather than allocated in memory. No support for other global qualifiers (uniform, in, out, buffer, etc.) in this plan.

Example:
```glsl
const float PI = 3.14159;
const float TWO_PI = 2.0 * PI;
const vec2 UNIT = vec2(1.0, 0.0);

float main() {
    return TWO_PI + length(UNIT);  // PI, TWO_PI, UNIT substituted at compile time
}
```

## Current State of the Codebase

### Parsing and AST
- **glsl crate** (light-player/glsl-parser, feature/spans) parses GLSL and produces `TranslationUnit` = list of `ExternalDeclaration`.
- `ExternalDeclaration` variants: `Preprocessor`, `FunctionDefinition`, `Declaration`.
- `const float PI = 3.14;` parses as `ExternalDeclaration::Declaration(Declaration::InitDeclaratorList(...))`.
- `InitDeclaratorList` has `head: SingleDeclaration` with `ty`, `name`, `initializer`, and optional `TypeQualifier` (which includes `StorageQualifier::Const`).

### Semantic Pipeline
- **CompilationPipeline** (`pipeline.rs`): parse → analyze → (optional) transform.
- **Semantic passes** only process `FunctionDefinition`; they **ignore** `ExternalDeclaration::Declaration` entirely.
- **TypedShader** has: `main_function`, `user_functions`, `function_registry`. **No globals.**
- **FunctionExtractionPass**, **FunctionRegistryPass**: iterate `shader.0`, only handle `FunctionDefinition`.

### Codegen / Variable Resolution
- **CodegenContext** has `variables: HashMap<String, VarInfo>` for function params and local decls.
- **Variable resolution** (`expr/variable.rs`, `lvalue/resolve/variable.rs`): `lookup_variable_type`, `lookup_variables`, `lookup_var_info` — all expect variables to exist in context from params/locals.
- **Array sizes** (`type_resolver.rs`): `parse_array_dimensions` only accepts `Expr::IntConst`; **const variables like `float arr[ADD_SIZE]` are rejected** with "array size must be a compile-time constant integer".

### Filetests
- `global/const-expression.glsl` — const globals (PI, TWO_PI, vectors, matrices). **All [expect-fail]**.
- `global/access-read.glsl` — reads from const (`CONST_FLOAT`), uniform, in, buffer, etc. **All [expect-fail]**.
- `array/declare-const-expression.glsl` — `const int ADD_SIZE = 2+3; float arr_add[ADD_SIZE];` — no [expect-fail], but type_resolver rejects const names in array dims.
- `array/declare-multidim-const.glsl`, `array/struct-array-const-size.glsl` — similar use of const in array sizes.

## Reference: DirectXShaderCompiler (HLSL)

- DXC evaluates **constant expressions** at compile time (SemaHLSL, const-expr.hlsl).
- Const locals (`const float sqrt_2 = 1.414...`) and const globals are folded.
- Array dimensions can use const integral expressions (e.g. `const uint sc_One = 1; float arr[sc_One];`).
- Const substitution: when a const is referenced, the compiler substitutes its evaluated value rather than emitting a load.

## Questions to Resolve

1. **Const expression subset**: Which expressions can appear in const initializers?
   - Suggested: literals, binary ops (+, -, *, /, %), unary (-), vector/matrix constructors, references to other consts. Defer: `length()`, `sin()` etc. (can add later if needed for const-init).

2. **Where to perform substitution**: Semantic pass (pre-codegen) vs. codegen-time lookup?
   - Suggested: Build a **ConstEnv** (name → evaluated value) in a semantic pass, pass it to codegen. On variable ref, check ConstEnv first; if present, emit literal. Keeps codegen simple and allows array-size resolution to use the same env.

3. **Ordering of const declarations**: `const B = A + 1` requires A to be defined first.
   - Suggested: Process `ExternalDeclaration::Declaration` in order; for `InitDeclaratorList` with Const qualifier, evaluate initializer (which may reference earlier consts), add to ConstEnv. Reject circular refs.

4. **Scope of const**: Global scope only for this plan?
   - Suggested: Yes. Local `const float x = 1.0;` inside functions is already handled as local decls — no change needed. This plan focuses on **global** const only.
