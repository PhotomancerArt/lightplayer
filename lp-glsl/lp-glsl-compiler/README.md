# lp-glsl-compiler - GLSL Compiler Frontend (Phase 1)

A GLSL fragment shader compiler using Cranelift JIT for the Light Player project.

## Phase 1 Status

**Phase 1 is complete!** ✅

This phase establishes the complete architecture and implements basic functionality.

### Implemented Features

- ✅ Full module structure and architecture
- ✅ Type system: int, bool, float, vec2/3/4, ivec2/3/4, bvec2/3/4
- ✅ Symbol table and scope management
- ✅ Semantic analysis infrastructure
- ✅ Cranelift codegen infrastructure
- ✅ Basic operations: arithmetic (+, -, \*, /), comparisons (==, !=, <, >, <=, >=)
- ✅ Variable declarations and assignments
- ✅ Control flow: if/else, for loops, while loops, break, continue, early return
- ✅ Vector operations: component access, swizzling, construction
- ✅ User-defined functions with parameters and return values
- ✅ Built-in functions:
  - Geometric: dot, cross, length, normalize, distance
  - Common math: min, max, clamp, abs, sqrt, floor, ceil
  - Common functions: mix, step, smoothstep, fract, mod, sign
- ✅ JIT compilation and execution
- ✅ Comprehensive file-based test suite

### Not Yet Implemented (Future Phases)

- ❌ Matrices (mat2, mat3, mat4)
- ❌ Structs
- ❌ Arrays
- ❌ Texture sampling functions
- ❌ Additional math functions (sin, cos, tan, exp, log, pow)
- ❌ Fixed-point transformation (in progress)

## Usage

```rust
use lp_glsl_compiler::Compiler;

fn main() {
    let mut compiler = Compiler::new();

    // Compile GLSL shader that returns an integer
    let shader = r#"
        int main() {
            int a = 10;
            int b = 32;
            return a + b;
        }
    "#;

    // Compile to native code
    let func = compiler.compile_int(shader).unwrap();

    // Execute and get result
    let result = func();
    assert_eq!(result, 42);

    println!("Shader returned: {}", result);
}
```

## Examples

See `examples/simple.rs` for more examples including:
- Integer arithmetic
- Boolean comparisons
- Complex expressions

Run the example:
```bash
cargo run -p lp-glsl-compiler --example simple
```

## Testing

The compiler includes a comprehensive test suite:

```bash
# Run all tests
cargo test -p lp-glsl-compiler
cargo test -p lp-glsl-filetests

# Run file-based tests
cd crates/lp-glsl-filetests
cargo test
```

Test coverage includes:
- Basic arithmetic and comparisons
- Control flow (if/else, loops, break, continue)
- Floating-point operations
- Vector operations (construction, swizzling, component access)
- User-defined functions
- Built-in functions (geometric, common math, interpolation)
- Fixed-point arithmetic
- Type error detection

## Architecture

### Module Structure

- `lib.rs` - Main entry point, `#![no_std]` with std feature gate
- `frontend.rs` - GLSL parsing (re-export from glsl-parser)
- `semantic/` - Semantic analysis and type checking
  - `types.rs` - Type system
  - `scope.rs` - Symbol table and scope management
  - `mod.rs` - Semantic validation pass
- `codegen/` - Cranelift IR code generation
  - `context.rs` - Code generation context
  - `expr.rs` - Expression handlers
  - `stmt.rs` - Statement handlers
  - `builtins.rs` - Built-in function implementations
- `jit.rs` - JIT compilation interface
- `compiler.rs` - Main compiler orchestration
- `transform/` - AST transformations
  - `fixed_point.rs` - Fixed-point arithmetic transformation

### Compilation Pipeline

```
GLSL Source
    ↓
[1] Parse (glsl-parser) → AST
    ↓
[2] Semantic Analysis → Typed AST
    ↓
[3] Cranelift Codegen → Cranelift IR
    ↓
[4] Cranelift Backend → Machine Code
    ↓
[5] JIT Execute → Runtime Validation
```

## Dependencies

- `glsl` - GLSL parser (from glsl-parser crate)
- `cranelift-frontend` - Frontend for Cranelift IR generation
- `cranelift-codegen` - Cranelift code generator
- `cranelift-jit` - JIT compiler (std feature only)
- `cranelift-module` - Module management
- `hashbrown` - Hash map implementation

## Features

- `std` (default) - Enable standard library support, includes JIT compilation
- `core` - Core functionality without std

## License

Apache-2.0 WITH LLVM-exception

