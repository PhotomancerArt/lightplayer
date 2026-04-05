# Filetest Annotation System — Design

## Scope

Replace the current filetest target/expect-fail system with a structured
annotation system. Tests become universal by default (run on all targets).
Annotations (`@unimplemented`, `@broken`, `@ignore`) with typed filters
control per-target behavior. The CLI uses the same filter mechanism.

Migrate all ~634 test files and update the gen-app.

## File Structure

```
lp-shader/lp-glsl-filetests/
└── src/
    ├── lib.rs                          # UPDATE: multi-target dispatch
    ├── parse/
    │   ├── mod.rs                      # UPDATE: collect annotations
    │   ├── parse_annotation.rs         # NEW: @unimplemented/@broken/@ignore parser
    │   ├── parse_run.rs                # UPDATE: remove [expect-fail] parsing
    │   ├── parse_target.rs             # UPDATE: parse new target format or absent
    │   ├── parse_test_type.rs          # unchanged
    │   ├── parse_expected_error.rs     # unchanged
    │   ├── parse_source.rs             # UPDATE: skip annotation lines from GLSL
    │   ├── parse_trap.rs               # unchanged
    │   └── test_type.rs                # UPDATE: new types (Annotation, TargetFilter, etc.)
    ├── target/
    │   ├── mod.rs                      # NEW: Target, TargetFilter, axis enums, DEFAULT_TARGETS
    │   └── display.rs                  # NEW: target name formatting (cranelift.q32 etc.)
    ├── test_run/
    │   ├── mod.rs                      # UPDATE: stats per target
    │   ├── run.rs                      # UPDATE: multi-target loop
    │   ├── run_summary.rs              # UPDATE: dispatch by Target instead of FiletestTarget
    │   ├── run_detail.rs               # UPDATE: dispatch by Target instead of FiletestTarget
    │   ├── target.rs                   # REMOVE: replaced by target/ module
    │   ├── wasm_runner.rs              # unchanged
    │   ├── execution.rs                # unchanged
    │   ├── parse_assert.rs             # unchanged
    │   └── test_glsl.rs                # unchanged
    ├── test_error/
    │   └── mod.rs                      # UPDATE: run once, ignore targets
    ├── runner/
    │   └── ...                         # unchanged
    └── util/
        ├── file_update.rs              # UPDATE: new annotation format
        └── ...                         # unchanged

lp-shader/lp-glsl-filetests-app/
└── src/
    └── main.rs                         # UPDATE: --target flag

lp-shader/lp-glsl-filetests-gen-app/
└── src/
    ├── util.rs                         # UPDATE: remove target from header
    └── vec/*.rs                        # UPDATE: remove target line from generators

lp-shader/lp-glsl-filetests/filetests/
├── scalar/**/*.glsl                    # UPDATE: remove // target, convert [expect-fail]
├── vec/**/*.gen.glsl                   # REGENERATE: via gen-app
├── builtins/**/*.glsl                  # UPDATE: remove // target, convert [expect-fail]
├── control/**/*.glsl                   # UPDATE: remove // target, convert [expect-fail]
├── ... (all other dirs)                # UPDATE: same treatment
└── wasm/int-add.glsl                   # MOVE: to scalar/int/ (no longer target-segregated)
```

## Conceptual Architecture

```
                    ┌────────────────────────┐
                    │    DEFAULT_TARGETS      │
                    │  ┌──────────────────┐   │
                    │  │ cranelift.q32    │   │
                    │  │ wasm.q32         │   │
                    │  └──────────────────┘   │
                    └───────────┬────────────┘
                                │
                    CLI --target filter
                                │
                    ┌───────────▼────────────┐
                    │   active target list    │
                    └───────────┬────────────┘
                                │
            ┌───────────────────▼───────────────────┐
            │          for each test file            │
            │                                        │
            │  ┌─ file-level annotations ──────────┐ │
            │  │ @unimplemented(backend=wasm)       │ │
            │  │ @ignore(...)                       │ │
            │  └───────────────────────────────────┘ │
            │                                        │
            │  for each target in active list:       │
            │    - check file annotations → skip?    │
            │    - compile once per target            │
            │                                        │
            │    for each // run: directive:          │
            │      ┌─ directive annotations ───────┐ │
            │      │ @unimplemented()              │ │
            │      │ @broken(isa=riscv32)          │ │
            │      └───────────────────────────────┘ │
            │      - check annotations → disposition │
            │        Skip / ExpectFailure / Normal   │
            │      - execute and compare             │
            └────────────────────────────────────────┘

    TargetFilter is the universal mechanism:
    - CLI uses it to narrow the target list
    - File-level @ignore/@unimplemented sets defaults
    - Directive-level annotations refine per-assertion
    - Same struct, same matching: all specified fields AND'd, None = wildcard
```

## Main Components

### Axis Enums (`target/mod.rs`)

```rust
enum Backend   { Cranelift, Wasm }
enum Isa       { Riscv32, Wasm32, Native }
enum ExecMode  { Jit, Emulator }
enum FloatMode { Q32, F32 }
```

### Target (`target/mod.rs`)

Concrete configuration — one point in the target space. All fields required.

```rust
struct Target {
    backend: Backend,
    float_mode: FloatMode,
    isa: Isa,
    exec_mode: ExecMode,
}
```

Predefined constants:

- `CRANELIFT_Q32`: cranelift, q32, riscv32, emulator
- `WASM_Q32`: wasm, q32, wasm32, emulator

Each has a canonical name (e.g., `"cranelift.q32"`) used in CLI and display.

### TargetFilter (`target/mod.rs`)

Partial target specification. `None` = wildcard. All specified fields AND'd.

```rust
struct TargetFilter {
    backend: Option<Backend>,
    float_mode: Option<FloatMode>,
    isa: Option<Isa>,
    exec_mode: Option<ExecMode>,
}

impl TargetFilter {
    fn matches(&self, target: &Target) -> bool { ... }
}
```

### Annotations (`parse/test_type.rs`)

```rust
enum AnnotationKind { Unimplemented, Broken, Ignore }

struct Annotation {
    kind: AnnotationKind,
    filter: TargetFilter,
    reason: Option<String>,
    line_number: usize,
}
```

Syntax: `// @kind(key=value, key=value, reason="...")`

### Annotation Parser (`parse/parse_annotation.rs`)

Parses lines matching `// @(unimplemented|broken|ignore)(...)`. Extracts
key=value pairs into a TargetFilter + optional reason string.

### TestFile updates (`parse/test_type.rs`)

```rust
struct TestFile {
    test_types: Vec<TestType>,
    glsl_source: String,
    annotations: Vec<Annotation>,        // file-level
    run_directives: Vec<RunDirective>,
    error_expectations: Vec<ErrorExpectation>,
    trap_expectations: Vec<TrapExpectation>,
    clif_expectations: ClifExpectations,
}

struct RunDirective {
    expression_str: String,
    comparison: ComparisonOp,
    expected_str: String,
    tolerance: Option<f32>,
    line_number: usize,
    annotations: Vec<Annotation>,        // directive-level (replaces expect_fail)
}
```

### Disposition logic

```rust
enum Disposition { ExpectSuccess, ExpectFailure, Skip }

fn directive_disposition(
    file_annotations: &[Annotation],
    directive_annotations: &[Annotation],
    target: &Target,
) -> Disposition {
    // Check directive-level first, then file-level
    for ann in directive_annotations.iter().chain(file_annotations.iter()) {
        if ann.filter.matches(target) {
            return match ann.kind {
                AnnotationKind::Ignore => Disposition::Skip,
                _ => Disposition::ExpectFailure,
            };
        }
    }
    Disposition::ExpectSuccess
}
```

### Runner changes

The runner iterates over active targets (DEFAULT_TARGETS filtered by CLI).
For each target × file × directive, it determines the disposition and acts
accordingly. Compilation happens once per (file, target) pair in summary mode.

### Error test handling

`test error` runs once regardless of target matrix. The frontend is shared.

### `test compile` / `test transform.q32` handling

Auto-skip for non-cranelift targets. The test type implies the backend.

### CLI

```
lp-glsl-filetests-app test [files...] [--target <name>] [--fix]

--target cranelift.q32   Run only cranelift.q32 target
--target wasm.q32        Run only wasm.q32 target
                         (omit for all targets)
--fix                    Remove annotations for tests that now pass
```

`--help` and errors list valid target names.

### File-level annotation syntax

```glsl
// test run
// @unimplemented(backend=wasm)

int test_add() { return 1 + 2; }
// run: test_add() == 3
```

Annotations between `// test run` and first GLSL code are file-level.

### Directive-level annotation syntax

```glsl
int test_bitfield() { return bitfieldInsert(15, 10, 4, 4); }
// @unimplemented()
// run: test_bitfield() == 175
```

Annotations immediately before a `// run:` line are directive-level.
Multiple annotations stack (each is an independent filter).

### Migration

All ~634 hand-written test files:

1. Remove `// target riscv32.q32`
2. Convert `[expect-fail]` suffix → `@unimplemented()` on preceding line
3. Files with features unsupported on wasm get file-level
   `@unimplemented(backend=wasm)`

Generated `.gen.glsl` files:

1. Update gen-app templates (remove target line)
2. Regenerate all files
