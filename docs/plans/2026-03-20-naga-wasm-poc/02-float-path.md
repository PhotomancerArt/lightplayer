# Phase 2: Float path — GLSL → Naga IR → f32 WASM → wasmtime

## Scope

Implement the core compilation pipeline for f32 mode. Parse a simple GLSL
function with Naga, walk the IR, emit WASM via wasm-encoder, and execute it
with wasmtime in a test.

Target GLSL:

```glsl
float add(float a, float b) {
    return a + b;
}
```

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation details

### 1. Public API in `src/lib.rs`

```rust
#![no_std]
extern crate alloc;

use alloc::vec::Vec;

pub enum NumericMode {
    Float,
    Q32,
}

pub struct CompileResult {
    pub wasm_bytes: Vec<u8>,
}

pub fn compile(source: &str, mode: NumericMode) -> Result<CompileResult, CompileError> {
    let module = parse_glsl(source)?;
    let wasm_bytes = emit_wasm(&module, &mode)?;
    Ok(CompileResult { wasm_bytes })
}
```

### 2. GLSL parsing wrapper

Use Naga's GLSL frontend. Naga requires a `ShaderStage` — use `Vertex` as a
dummy (we're compiling free functions, not actual shader stages). The key
concern is whether Naga accepts non-entry-point functions.

If Naga requires an entry point, we may need to wrap the input as:

```glsl
#version 450
float add(float a, float b) {
    return a + b;
}
void main() {}  // dummy entry point if needed
```

And then extract the non-main function from `module.functions` (as opposed to
`module.entry_points`).

```rust
fn parse_glsl(source: &str) -> Result<naga::Module, CompileError> {
    let mut frontend = naga::front::glsl::Frontend::default();
    let options = naga::front::glsl::Options::from(naga::ShaderStage::Vertex);
    frontend.parse(&options, source).map_err(|e| CompileError::Parse(e))
}
```

### 3. WASM emitter

Walk the first non-main function in `module.functions`. The emitter needs to:

**a) Map function signature to WASM types:**
- `float` param → `ValType::F32`
- `float` return → `[ValType::F32]`

**b) Allocate locals:**
- Params get indices 0..n_params (implicit in WASM)
- Each expression in the arena gets a local. We only need locals for
  expressions that produce values and are referenced. For the spike, allocate
  one f32 local per expression handle (wasteful but correct).
- Declare `arena.len() - n_params` additional f32 locals after params.

Actually, simpler: use a stack-based approach. WASM is a stack machine.
For `a + b`:
1. `local.get 0` (a)
2. `local.get 1` (b)
3. `f32.add`
No extra locals needed for this case. The expression evaluation pushes results
onto the WASM stack.

For the spike, implement a recursive `emit_expr(handle)` that pushes one value
onto the stack:

```rust
fn emit_expr(func: &naga::Function, handle: Handle<Expression>, sink: &mut Function) {
    match func.expressions[handle] {
        Expression::FunctionArgument(idx) => {
            sink.instruction(&Instruction::LocalGet(idx));
        }
        Expression::Binary { op, left, right } => {
            emit_expr(func, left, sink);
            emit_expr(func, right, sink);
            match op {
                BinaryOperator::Add => sink.instruction(&Instruction::F32Add),
                // ... extend as needed
            }
        }
        _ => panic!("unsupported expression: {:?}", ...),
    }
}
```

**c) Walk body statements:**

```rust
for stmt in &func.body {
    match stmt {
        Statement::Emit(_range) => {
            // In Naga, Emit marks which expressions to evaluate.
            // For our stack-based approach, we handle evaluation lazily
            // when an expression is referenced, so Emit is a no-op.
        }
        Statement::Return { value: Some(expr) } => {
            emit_expr(func, *expr, sink);
            sink.instruction(&Instruction::Return);
        }
        _ => panic!("unsupported statement"),
    }
}
```

**d) Assemble WASM module:**

Use `wasm_encoder::Module`, `TypeSection`, `FunctionSection`, `ExportSection`,
`CodeSection` to build a valid WASM module with one exported function.

### 4. Test in `tests/smoke.rs`

```rust
#[test]
fn test_float_add() {
    let source = r#"
        #version 450
        float add_floats(float a, float b) {
            return a + b;
        }
        void main() {}
    "#;
    let result = naga_wasm_poc::compile(source, naga_wasm_poc::NumericMode::Float).unwrap();

    let engine = wasmtime::Engine::default();
    let mut store = wasmtime::Store::new(&engine, ());
    let module = wasmtime::Module::new(&engine, &result.wasm_bytes).unwrap();
    let instance = wasmtime::Instance::new(&mut store, &module, &[]).unwrap();
    let func = instance
        .get_func(&mut store, "add_floats")
        .unwrap()
        .typed::<(f32, f32), f32>(&store)
        .unwrap();
    let result = func.call(&mut store, (1.5, 2.5)).unwrap();
    assert_eq!(result, 4.0);
}
```

## Validate

```bash
cargo test -p naga-wasm-poc
```

The `test_float_add` test must pass — proving GLSL → Naga IR → WASM → correct
execution.
