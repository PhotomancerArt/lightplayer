# Design — `lpir-inliner` stage ii (M1 `CompilerConfig` + filetest `compile-opt`)

## Scope of work

Implement **M1** from `docs/roadmaps/2026-04-15-lpir-inliner/m1-optpass-filetests.md`:

- Introduce **`lpir::CompilerConfig`** (and **`InlineConfig`**, **`InlineMode`**, **`ConfigError`**) as **`no_std` + `alloc`** middle-end options for LPIR optimization passes.
- Thread **`config: CompilerConfig`** through **`lpvm-native`**, **`lpvm-cranelift`**, and **`lpvm-wasm`** option structs; **`Default`** uses **`CompilerConfig::default()`**.
- Add filetest directive **`// compile-opt(key, value)`**, **`TestFile::config_overrides`**, duplicate-key errors, and merge overrides before compilation in **`filetest_lpvm`** for **all** backends.

**No behavior change** for existing tests until files add **`compile-opt`** and later milestones wire the inliner to read **`InlineConfig`**.

**Out of scope:** M0 **`CalleeRef`** refactor (parallel plan); inliner body; tagging **`filetests/function/*.glsl`** with **`compile-opt`** (optional follow-up).

See **`00-notes.md`** for resolved questions.

## Implementation granularity

Prefer **keeping the workspace building and tests passing after each phase** (additive `Default` fields and plumbing). If M0 lands in parallel and causes transient conflicts, resolve before declaring the plan complete.

## File structure (relevant areas)

```
lp-shader/lpir/src/
├── compiler_config.rs          # NEW: CompilerConfig, InlineConfig, InlineMode, ConfigError, apply, FromStr
└── lib.rs                      # UPDATE: mod + re-exports

lp-shader/lpvm-native/src/
├── native_options.rs           # UPDATE: + config; Clone not Copy
├── compile.rs                  # UPDATE: pass options.config where passes need it (inline = later; may no-op for M1)
└── …                           # UPDATE: any NativeCompileOptions { … } literals

lp-shader/lpvm-cranelift/src/
├── compile_options.rs          # UPDATE: + config; likely Clone only
└── …                           # UPDATE: struct literals, engine paths

lp-shader/lpvm-wasm/src/
├── options.rs                  # UPDATE: + config; likely Clone only
└── …

lp-shader/lps-filetests/src/parse/
├── parse_compile_opt.rs        # NEW: // compile-opt(key, value)
├── mod.rs                      # UPDATE: try compile-opt before @ annotations; duplicate keys
├── test_type.rs                # UPDATE: TestFile::config_overrides
└── parse_annotation.rs         # (unchanged kinds — no Config on AnnotationKind)

lp-shader/lps-filetests/src/test_run/
└── filetest_lpvm.rs           # UPDATE: build CompilerConfig, set on FaCompileOptions, CompileOptions, WasmOptions

lp-shader/lps-frontend / lp-engine / fw / tests
└── UPDATE: any ..Default::default() or struct copies that assumed Copy on option structs
```

## Conceptual architecture

```
┌──────────────────────────────────────────────────────────────────┐
│  lps-frontend (GLSL → LPIR)                                        │
└────────────────────────────┬─────────────────────────────────────────┘
                             ▼
┌──────────────────────────────────────────────────────────────────┐
│  LPIR module                                                        │
│  ─────────────────────────────────────────────────────────────── │
│  CompilerConfig  ← middle-end: inline mode, budgets, future passes  │
│       ▲                                                             │
│       │  filetest: // compile-opt(k, v) → apply() on defaults        │
│       │  production: NativeCompileOptions / CompileOptions / …     │
└───────┼────────────────────────────────────────────────────────────┘
        │
        ▼  LPIR passes (const_fold today; inline when wired) read config
┌──────────────────────────────────────────────────────────────────┐
│  Backend lowering                                                  │
│  NativeCompileOptions │ CompileOptions │ WasmOptions                │
│  (+ float_mode, emu_trace, q32_options, … per backend)             │
└──────────────────────────────────────────────────────────────────┘
```

**Separation:** **`CompilerConfig`** does not subsume backend flags ( **`FloatMode`**, debug, WASM-only knobs). It only groups **shared LPIR pass** settings so every codegen path sees the same middle-end choices.

## Main components and interactions

| Piece | Role |
|-------|------|
| **`CompilerConfig::apply`** | Single namespace for **`compile-opt`** string keys → field updates; unknown key / bad value → error |
| **`TestFile::config_overrides`** | Raw **`(key, value)`** from file; duplicate keys rejected in **`parse_test_file`** |
| **`CompiledShader::compile_glsl`** | After **`lower_glsl`**, merge overrides into **`CompilerConfig::default()`**, install on each backend’s options before **`compile`** |

## Phases

1. **`01-lpir-compiler-config.md`** — `compiler_config.rs`, tests for **`apply`** / **`InlineMode::from_str`**
2. **`02-thread-config-through-backends.md`** — **`NativeCompileOptions`**, **`CompileOptions`**, **`WasmOptions`**, fix **`Copy`/`Clone`** and all call sites
3. **`03-filetests-compile-opt.md`** — parsing, **`TestFile`**, **`filetest_lpvm`** wiring
4. **`04-cleanup-and-validation.md`** — diff hygiene, full test matrix, **`summary.md`**, move to **`plans-done/`**, commit template
