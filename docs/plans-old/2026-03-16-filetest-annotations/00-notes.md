# Filetest Annotation System — Notes

## Scope of Work

Replace the current filetest target/expect-fail system with a structured
annotation system that:

1. Makes tests universal by default (no `// target` required)
2. Introduces typed annotations (`@unimplemented`, `@broken`, `@ignore`) with
   filter parameters (backend, isa, exec_mode, float_mode)
3. Supports file-level and directive-level annotations
4. Uses the same `TargetFilter` mechanism for CLI filtering, file-level
   restrictions, and directive-level expectations
5. Migrates all ~634 test files to the new format

## Current State

- Every test file has `// target riscv32.q32` (1 wasm test exists)
- `[expect-fail]` is an inline suffix on `// run:` lines, unqualified
- ~900+ `[expect-fail]` annotations across the test suite (all cranelift-only,
  meaning "unimplemented in cranelift")
- Parser: `parse/parse_target.rs` extracts target string,
  `parse/parse_run.rs` handles `[expect-fail]`
- Runner: `test_run/target.rs` maps target strings to `FiletestTarget` enum
  (Cranelift or Wasm), `run_summary.rs` and `run_detail.rs` dispatch by target
- CLI: `lps-filetests-app` has `--fix` flag; `LP_FIX_XFAIL=1` and
  `LP_MARK_FAILING_TESTS_EXPECTED=1` env vars for bulk annotation management
- Gen-app: generates `.gen.glsl` files with `// target riscv32.q32` baked in

## Agreed Design

### Axis Enums

```rust
enum Backend    { Cranelift, Wasm }
enum Isa        { Riscv32, Wasm32, Native }
enum ExecMode   { Jit, Emulator }
enum FloatMode  { Q32, F32 }
```

### Target (concrete configuration)

```rust
struct Target {
    backend: Backend,
    float_mode: FloatMode,
    isa: Isa,
    exec_mode: ExecMode,
}
```

Predefined constants: `Target::CRANELIFT_Q32`, `Target::WASM_Q32`, etc.

### TargetFilter (partial, for annotations and CLI)

```rust
struct TargetFilter {
    backend: Option<Backend>,
    float_mode: Option<FloatMode>,
    isa: Option<Isa>,
    exec_mode: Option<ExecMode>,
}
```

Matching: all specified fields AND'd, `None` = wildcard.

### Annotations

```rust
enum AnnotationKind { Unimplemented, Broken, Ignore }

struct Annotation {
    kind: AnnotationKind,
    filter: TargetFilter,
    reason: Option<String>,
    line_number: usize,
}
```

Syntax in test files:
```
// @unimplemented()
// @unimplemented(backend=wasm)
// @broken(isa=riscv32, reason="overflow in emulator")
// @ignore(backend=wasm)
```

File-level: annotations between `// test run` and first GLSL code.
Directive-level: annotations immediately before a `// run:` line.
Stacking: file-level + directive-level both apply (independent filters).

### Behavior

- `@unimplemented` / `@broken` → xfail (run, expect failure, flag unexpected pass)
- `@ignore` → skip (don't compile or run)

## Questions

### Q1: Default target matrix

What should the default target matrix be after this refactor?

Current state: only cranelift.q32 exists as a working target. wasm.q32 is being
built in part-ii.

Options:
- (a) `[cranelift.q32]` only — add wasm.q32 later when the wasm backend lands
- (b) `[cranelift.q32, wasm.q32]` — include wasm now, with all tests
  file-level `@unimplemented(backend=wasm)` until wasm supports them

Suggestion: (a) — ship with cranelift.q32 only. Add wasm.q32 to the default
matrix as part of the wasm work. This keeps the annotation refactor
independent from the wasm work.

Answer: (b) — both cranelift.q32 and wasm.q32. The wasm backend is built and
integrated (lps-wasm crate, wasmtime runner, one passing test). The whole
point of this refactor is to support multi-target. Most existing tests will
need file-level `@unimplemented(backend=wasm)` until wasm support grows.

### Q2: CLI filter syntax

How should the runner CLI accept target filters?

Options:
- (a) Named flags: `--backend wasm --float-mode q32`
- (b) Single filter flag: `--filter backend=wasm,float_mode=q32`
- (c) Shorthand target flag: `--target cranelift.q32` (maps to a predefined
  Target, not a free-form filter)

Suggestion: (c) for the common case plus (a) for fine-grained control. Most
of the time you want `--target cranelift.q32` or `--target wasm.q32`. The
named flags are for edge cases.

Answer: (c) — `--target cranelift.q32` / `--target wasm.q32`. Each predefined
Target gets a dotted name (`backend.float_mode`). `--help` and error messages
list valid target names. Per-axis flags (--backend, etc.) not needed initially.

### Q3: DecimalFormat → FloatMode rename scope

`DecimalFormat` exists in `lps-cranelift` and `lps-wasm` as a public
type. Should we rename it to `FloatMode` everywhere, or only in the filetests
crate?

Options:
- (a) Rename everywhere (cranelift, wasm, filetests) — consistent naming
- (b) Rename only in filetests, keep DecimalFormat in cranelift/wasm —
  less churn, but two names for the same concept
- (c) Keep DecimalFormat everywhere, use it in filetests too — no rename at
  all

Suggestion: (a) — rename everywhere. It's mechanical and you have tools for it.
FloatMode is more accurate than DecimalFormat.

Answer: (a) — rename DecimalFormat→FloatMode and Float→F32 everywhere (frontend,
cranelift, wasm, filetests). User will do this rename separately as a
mechanical step. Not a core part of this plan's phases but a prerequisite.

### Q4: Error tests and multi-target

`// test error` tests check frontend diagnostics. The frontend is shared
across backends. Should error tests run once (backend-independent) or once
per target?

Options:
- (a) Run once, ignore targets — error tests are frontend-only
- (b) Run per target — future-proofs for backend-specific error messages

Suggestion: (a) for now. Error tests don't compile to machine code, so the
backend is irrelevant. If backend-specific errors emerge later, we can change
the behavior.

Answer: (a) — run once, ignore targets. Error tests exercise the frontend only.

### Q5: test compile / test transform.q32 with multi-target

These test types check Cranelift IR, which is inherently backend-specific.
Should they require a `@target(backend=cranelift)` annotation? Or should the
runner automatically skip them for non-cranelift targets?

Suggestion: The runner should know that `test compile` and `test transform.q32`
are cranelift-specific and auto-skip them for other backends. No annotation
needed in the test file.

Answer: (b) — auto-skip for non-cranelift targets. The test type implies the
backend. No annotation needed.

### Q6: Migration of existing [expect-fail]

All ~900 existing `[expect-fail]` annotations are on cranelift.q32 (the only
backend). They represent unimplemented features in cranelift. What should they
become?

Options:
- (a) `@unimplemented()` — no filter, meaning "unimplemented everywhere"
- (b) `@unimplemented(backend=cranelift)` — specific to cranelift
- (c) Keep as-is until we need to change them

Suggestion: (a) — these are features not implemented in any backend. When wasm
arrives, they'll also be unimplemented there. `@unimplemented()` is accurate.

Answer: (a) — `@unimplemented()` with no filter. These features are unimplemented
on all backends. When a feature is implemented on one backend but not another,
update the annotation to be backend-specific at that point.

### Q7: Gen-app updates

The gen-app generates `.gen.glsl` files with `// target riscv32.q32`. What
changes?

Suggestion: Remove the target line from generated files. If any generated tests
have `[expect-fail]`, convert to `@unimplemented()`. The gen-app templates
need updating.

Answer: Update gen-app templates as a phase in this plan. Remove target line,
emit @unimplemented() instead of [expect-fail], add file-level
@unimplemented(backend=wasm) for vector/matrix tests. Regenerate all
.gen.glsl files.

## Notes

- FloatMode rename (DecimalFormat→FloatMode, Float→F32) is a prerequisite
  that the user will do separately as a mechanical rename.
- The wasm/int-add.glsl test in filetests/wasm/ should be moved to a
  regular location (e.g., scalar/int/) since tests are no longer
  organized by target.
- The `@target(...)` file-level directive for positive selection is
  available but expected to be rare. Most tests have no target restriction.

(To be populated during question iteration)
