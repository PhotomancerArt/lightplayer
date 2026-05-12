# GLSL → WASM Playground: Design

## Goal

A web page that compiles GLSL shader source to WASM and executes it
in the browser, rendering output to a canvas. This validates the
GLSL → WASM compilation path end-to-end before integrating it into
the larger LightPlayer web app.

Target: render `basic/rainbow.shader` in a browser with no server.

## Crate Structure

```
lp-shader/
├── lps-frontend/              # NEW: shared parser + semantic analysis
│   ├── Cargo.toml                 #   deps: glsl, hashbrown, serde, log, lp-model
│   └── src/
│       ├── lib.rs
│       ├── error.rs
│       ├── pipeline.rs            #   CompilationPipeline (parse + analyze)
│       ├── semantic/              #   18 files: TypedShader, types, type_check, etc.
│       ├── src_loc.rs
│       └── src_loc_manager.rs
│
├── lps-cranelift/             # RENAMED from lps-compiler
│   ├── Cargo.toml                 #   deps: lps-frontend, cranelift-*, lps-builtins
│   └── src/
│       ├── lib.rs                 #   glsl_jit, glsl_jit_streaming, glsl_emu_riscv32
│       ├── codegen/               #   60 files (unchanged CLIF emission)
│       ├── module/                #   GlModule<JIT|Object>
│       ├── builtins/              #   BuiltinId registry, linker
│       ├── target/                #   Target ISA config
│       └── exec/                  #   GlslExecutable, execute_fn, etc.
│
├── lps-wasm/                  # NEW: WASM codegen backend
│   ├── Cargo.toml                 #   deps: lps-frontend, wasm-encoder
│   └── src/
│       ├── lib.rs                 #   glsl_wasm(source, options) → WasmShaderModule
│       ├── codegen/               #   AST walker → WASM bytecode
│       │   ├── context.rs         #     WasmCodegenContext (locals, imports)
│       │   ├── module.rs          #     WASM module builder
│       │   ├── expr/              #     Expression emission
│       │   ├── stmt/              #     Statement emission
│       │   ├── lvalue/            #     LValue handling
│       │   ├── builtins.rs        #     Builtin import declarations
│       │   └── numeric.rs         #     NumericMode → WASM ops
│       └── types.rs               #     WasmShaderModule output type
│
├── lps-builtins/              # EXISTING (unchanged, also compiled to .wasm)
├── lps-filetests/             # EXISTING: extended with wasmtime runner
├── lps-jit-util/              # EXISTING (unchanged, cranelift-only)
└── ...

lp-app/
└── playground/                    # NEW: web playground
    ├── Cargo.toml                 #   deps: lps-frontend, lps-wasm, wasm-bindgen
    ├── src/
    │   └── lib.rs                 #   wasm-bindgen API: compile(source) → WasmBytes
    └── www/
        └── index.html             #   textarea, canvas, compile button, JS glue
```

## Architecture

```
                    lps-frontend
                   (parse → TypedShader)
                    /              \
       lps-cranelift        lps-wasm
    (TypedShader → CLIF →      (TypedShader →
     native/rv32)               WASM bytes)
          |                        |
    lps-jit-util          wasm-encoder
    cranelift-*                    |
          |                   ┌────┴────┐
     native exec              │ browser │
     rv32 emulator            │ wasmtime│
                              └─────────┘
                                   ↑
                          lps-builtins.wasm
                          (imports at instantiation)
```

### Key architectural properties

**Shared frontend**: lps-frontend owns parsing, semantic analysis, and
the TypedShader representation. Both backends depend on it. No Cranelift
types in the shared code.

**Parallel backends**: lps-cranelift and lps-wasm are symmetric.
Both walk the same TypedShader AST. Both use the same pluggable NumericMode
architecture (Q32 first, float later). They produce different output
(native code vs WASM bytes) but share the same compilation options.

**WASM import-based builtin linking**: lps-builtins compiles to a
`.wasm` binary. Shader WASM modules declare builtins as imports. At
instantiation, the builtins module's exports are provided as the import
object. Type-safe linking at instantiation time. Same pattern in the
browser and in wasmtime tests.

**No Cranelift in the WASM dependency tree**: The playground depends on
lps-frontend + lps-wasm + lps-builtins. No cranelift-*
crates are pulled in.

### WASM codegen simplifications vs Cranelift backend

- No SSA construction (WASM locals instead of SSA values)
- No block sealing (structured control flow from AST)
- No struct return (WASM multi-value return for vec4)
- No pointer type variation (always i32 / wasm32)
- No calling convention complexity (WASM has one)

### Playground execution flow

1. Page loads compiler WASM (lps-frontend + lps-wasm) and
   builtins WASM
2. User edits GLSL source in textarea, clicks "compile"
3. JS calls compiler: GLSL source → WASM bytes
4. JS calls WebAssembly.instantiate(shaderBytes, { builtins: builtinsExports })
5. requestAnimationFrame loop: call shader main() per pixel, write to
   ImageData, putImageData to canvas
6. Compiler errors displayed in output panel

### Filetest architecture

The existing filetest infrastructure is extended with a pluggable runtime:

- Cranelift runtime: existing native execution
- WASM runtime: wasmtime execution (test-only dependency)
- Per-target `[expect-fail]` annotations for tests that the WASM backend
  doesn't support yet
- "target riscv32.q32" directive is obsolete and ignored
- All tests applicable to all targets; what varies is pass/fail expectations

### Rendering model

64x64 pixel grid, scaled 2-4x on canvas. Shader called per-pixel with
(fragCoord, outputSize, time). Output vec4 → RGBA. Animated via
requestAnimationFrame.
