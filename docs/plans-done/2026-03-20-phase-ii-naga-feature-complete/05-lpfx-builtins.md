# Phase 5: LPFX builtins + prototype injection

## Scope

Enable LPFX (`lpfx_*`) function calls by injecting GLSL prototypes into
`lp-glsl-naga` and emitting them as WASM imports in `lp-glsl-wasm`. This
unblocks the 13 `lpfx/` test files.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Create lp-glsl-naga/src/builtins.rs

Define GLSL forward declarations for all known LPFX functions. These must
match the signatures used by `lp-glsl-builtins-wasm`.

```rust
/// GLSL forward declarations for all LPFX builtin functions.
/// Prepended to user source so Naga can parse calls to them.
pub const LPFX_PROTOTYPES: &str = r#"
float lpfx_psrdnoise(vec2 pos, vec2 per, float rot, out vec2 gradient, uint quality);
float lpfx_worley(vec2 pos, uint quality);
float lpfx_fbm(vec2 pos, int octaves, uint quality);
float lpfx_simplex1(float pos, uint quality);
float lpfx_simplex2(vec2 pos, uint quality);
float lpfx_simplex3(vec3 pos, uint quality);
float lpfx_gnoise(vec2 pos, uint quality);
float lpfx_random(vec2 pos, uint quality);
float lpfx_srandom(vec2 pos, uint quality);
float lpfx_hash(float pos, uint quality);
float lpfx_saturate(float x);
vec3 lpfx_hsv2rgb(vec3 hsv);
vec3 lpfx_rgb2hsv(vec3 rgb);
vec3 lpfx_hue2rgb(float hue);
"#;
```

**Important**: Check these signatures against the actual exports in
`lp-glsl-builtins-wasm`. The parameter types and counts must match exactly.

Also check the actual LPFX filetest source to see what signatures they use.

### 2. Update lp-glsl-naga compile() for prototype injection

```rust
pub fn compile(source: &str) -> Result<NagaModule, CompileError> {
    let source = prepend_builtins(source);
    let source = ensure_vertex_entry_point(&source);
    let module = parse_glsl(&source)?;
    let functions = extract_functions(&module)?;
    Ok(NagaModule { module, functions })
}

fn prepend_builtins(source: &str) -> String {
    let mut s = String::from(builtins::LPFX_PROTOTYPES);
    s.push_str("#line 1\n");
    s.push_str(source);
    s
}
```

### 3. Verify `#line 1` works with Naga's GLSL frontend

Naga's `pp-rs` preprocessor must handle `#line`. Test by compiling a simple
shader with prototypes prepended and checking that error line numbers are
correct.

If `#line` is not supported by `pp-rs`, fall back to counting prototype
lines and adjusting error offsets manually.

### 4. LPFX function detection in lp-glsl-wasm

In `imports.rs`, when scanning for required imports, also scan for
`Statement::Call` where the called function's name starts with `lpfx_`.

Map LPFX function names to import names:
- `lpfx_psrdnoise` → `__lp_psrdnoise`
- `lpfx_worley` → `__lp_worley`
- etc.

The naming convention: strip `lpfx_` prefix, add `__lp_` prefix.

### 5. LPFX function call emission

LPFX calls appear as `Statement::Call { function, arguments, result }` where
`function` is a `Handle<Function>` pointing to the prototype function in
`naga::Module::functions`.

In `emit_call.rs`, detect LPFX functions by name:

```rust
pub fn emit_call(
    module: &Module,
    func: &Function,
    target: Handle<Function>,
    arguments: &[Handle<Expression>],
    result: Option<Handle<Expression>>,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
    func_index_map: &BTreeMap<Handle<Function>, u32>,
    imports: &ImportTable,
) -> Result<(), String> {
    let called = &module.functions[target];
    let name = called.name.as_deref().unwrap_or("");

    if name.starts_with("lpfx_") {
        return emit_lpfx_call(name, arguments, result, module, func,
            wasm_fn, mode, alloc, imports);
    }

    emit_user_call(target, arguments, result, module, func,
        wasm_fn, mode, alloc, func_index_map)
}
```

### 6. `out` parameter handling for LPFX

`lpfx_psrdnoise` has an `out vec2 gradient` parameter. In Naga's IR,
`out` parameters are modeled as pointer arguments. The caller passes a
`LocalVariable` pointer, and the callee writes through it.

For WASM imports, `out` parameters must be passed via linear memory (the
import function writes results to a memory address). Alternatively, the
WASM import can return the out values as additional return values.

Check how `lp-glsl-builtins-wasm` handles `out` parameters. The old backend
likely passed them via `env.memory`.

If the builtins use `env.memory`:
1. Allocate a memory region (e.g. a fixed address) for out parameters
2. Pass the memory offset as an i32 argument
3. After the call, load the results from memory

If the builtins use multi-value returns:
1. The import type includes extra return values for out params
2. After the call, store the extra results to the caller's local variables

### 7. Exclude LPFX prototypes from exports

The prepended LPFX prototypes create functions in `naga::Module::functions`.
`extract_functions()` currently exports all named functions. Filter out
LPFX functions:

```rust
fn extract_functions(module: &Module) -> Result<Vec<(Handle<Function>, FunctionInfo)>, CompileError> {
    let mut out = Vec::new();
    for (handle, function) in module.functions.iter() {
        let Some(name) = function.name.clone() else { continue; };
        if name == "main" || name.starts_with("lpfx_") {
            continue;
        }
        // ...
    }
    Ok(out)
}
```

## Validate

```bash
scripts/glsl-filetests.sh --target wasm.q32 "lpfx/"
scripts/glsl-filetests.sh --target wasm.q32
cargo check -p lp-glsl-wasm
cargo check -p lp-glsl-naga
```

All LPFX filetests should pass (or have known tolerance differences annotated).
