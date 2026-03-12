# Plan: Filetest Compile-Error Test Support

## Scope of Work

Add support for **`// test error`** in the GLSL filetest harness. Tests that expect a compile error should:

1. Be discovered and run (currently they are effectively ignored)
2. Attempt to compile the GLSL
3. Fail if compilation succeeds (expected error, got success)
4. When compilation fails, verify the error matches expectations:
   - `EXPECT_ERROR_CODE` – error code (e.g. E0400)
   - `EXPECT_ERROR` – message substring
   - `EXPECT_LOCATION` – optional source line number

## Current State of the Codebase

### Test type parsing

- `parse_test_type.rs`: Only matches `// test compile`, `// test transform.q32`, `// test run`. `// test error` is not handled.
- `TestType` enum in `test_type.rs`: Has `Compile`, `TransformQ32`, `Run`. No `Error` variant.
- A file with only `// test error` ends up with `test_types: []` (empty) because `parse_test_type` returns `None` for that line.

### Runner flow (`lib.rs`)

- `run_filetest_with_line_filter` checks `Compile`, `TransformQ32`, then `Run`.
- If none match, the else branch (lines 111–117) returns `Ok(())` with default stats.
- Result: error tests are silently passed without running anything.

### Existing error tests (`filetests/type_errors/`)

Three files already use the intended directives:

- `incdec-non-lvalue.glsl`
- `incdec-bool.glsl`
- `incdec-nested.glsl`

Each has:

- `// test error`
- `// target riscv32.q32`
- `// EXPECT_ERROR_CODE: E0xxx`
- `// EXPECT_ERROR: <message substring>`
- `// EXPECT_LOCATION: <line>` (optional)

### Compiler error format

- `lp-glsl-compiler` uses `GlslError` with `ErrorCode` enum (e.g. E0001, E0100, E0112, E0400).
- Display format: `error[E0100]: message`
- `GlslError` has `code`, `message`, `location` (with line number via `GlSourceLoc`).
- `glsl_emu_riscv32_with_metadata` returns `Result<..., GlslError>` on failure.

### Parsing

- `parse/mod.rs` collects directives in a first pass; trap expectations via `parse_trap`.
- There is no parser for `EXPECT_ERROR`, `EXPECT_ERROR_CODE`, `EXPECT_LOCATION`.
- `parse_source.rs` skips `// test` and `// target` when extracting GLSL source; same handling would apply for error tests.

### DirectX Shader Compiler reference

- Uses Clang-style `expected-error {{message}}` in source comments.
- VerifierHelper.py: regex `rxDiag = re.compile(r"((expected|fxc)-(error|warning|note|pass)\s*\{\{(.*?)\}\}\s*)")` for `expected-error {{...}}`.
- Their approach: compile, compare diagnostics to expected patterns. Our approach: similar, but with our own directive syntax.

## DirectX / Clang Verify Approach (reference)

DXC uses **Clang's -verify mode**, which works like this:

1. **RUN line**: `// RUN: %clang_cc1 -fsyntax-only ... -verify %s` — `-verify` means "check expected diagnostics"
2. **Inline expectations**: `// expected-error {{message}}` as **line-ending comments** on the line where the error occurs
3. **Implicit location**: The expectation is on the same line as the offending code, so location is derived from the comment's line
4. **Multiple per line**: One line can have `expected-error {{a}} expected-error {{b}}` if multiple errors
5. **Other directives**: `expected-warning`, `expected-note`, `expected-error-re` (regex)
6. **Line offset syntax**: `// expected-error@+1 {{...}}` means the error is on the *next* line
7. **One file, one verify run**: The whole file is compiled once; all expected-error comments are matched against all diagnostics

Example from `cpp-errors.hlsl`:
```hlsl
float f_arr_empty_init[] = { 1, 2, 3 };
float f_arr_empty_pack[] = { 1, 2 ... }; // expected-error {{expansion is unsupported in HLSL}}

s_arr_i_f arr_struct_incomplete[] = { 1, 2, 3 }; // expected-error {{too few elements in vector initialization (expected 4 elements, have 3)}}
```

**Key difference**: No separate "test error" mode. The file is a verify test if the RUN line includes `-verify`. Expectations are always inline.

## Alternative: Inline + Per-Test Design

User preference: **inline expectations** (like Clang) and **per-test** semantics.

Options:

**A) Clang-style verify**  
- Add `// test verify` or `-verify`-style RUN that means "compile and match inline expectations"
- `// expected-error {{...}}` on any line; optional `// expected-error-code: E0400`
- Runner: scan file for all expected-error comments, compile, get diagnostics with locations, match each diagnostic to the corresponding line's expectations
- One "test" per file, but N assertions (one per expected-error comment)

**B) Per-function error tests with inline expectations**  
- `// error run:` or similar marks a block/function as "should fail to compile"
- Expectations inline on the offending line: `// expected-error {{...}}`
- Allows mixing: some `// run:` (execute tests), some `// error run:` (compile-fail tests) in same file
- Each error run = one test case (like each `// run:` is one test case)

**C) Hybrid**  
- `// test error` at file level (or we infer from presence of expected-error)
- Inline `// expected-error {{...}}` and optionally `// expected-error-code: E0xxx` on the line
- File-level `// test error` means "this file should fail to compile; match inline expectations"
- Simpler than (A) if we only support single-error files for now

## Questions

### Q1: Inline vs block-level expectations (design choice)

**Context**: Current type_errors tests use block-level directives at the end of the file. User prefers **inline expectations** (like Clang/DXC) and **per-test** semantics.

**Question**: Which design do we want?

- **Option A – Clang-style verify**: `// test verify` + inline `// expected-error {{...}}` on each line that should produce an error. One compile, match all expectations. Supports multiple errors per file.
- **Option B – Per-run error tests**: Something like `// error run:` that marks a test case which should fail to compile, with inline `// expected-error {{...}}` on the offending line. Parallels `// run:` for execution tests.
- **Option C – Hybrid**: Keep `// test error` at file level, but move expectations inline (on the line with the error). Simpler, one error per file for now.

**Suggested answer**: Option A (Clang-style) is most flexible and matches DXC. Option B aligns with our existing `// run:`-per-test model. Option C is minimal change from current tests. Recommend Option A for flexibility, or Option B if we want strict symmetry with run tests.

---

### Q2: EXPECT_ERROR matching: substring vs exact

**Context**: `EXPECT_ERROR` will be matched against the compiler error message.

**Question**: Should `EXPECT_ERROR` require an exact match or a substring match?

**Suggested answer**: Substring match. This keeps tests stable when the compiler adds context (e.g. spec references) and aligns with common practice (e.g. Clang, DXC). The existing tests use descriptive substrings like "increment/decrement only supported on variables and vector components for now".

---

### Q3: EXPECT_LOCATION handling

**Context**: `GlslError.location` is `Option<GlSourceLoc>`, which includes a line number. The tests use `// EXPECT_LOCATION: 4` (or 6) to indicate the expected line.

**Question**: Should `EXPECT_LOCATION` be required when the compiler provides a location, or always optional?

**Suggested answer**: Optional. If `EXPECT_LOCATION` is present and the compiler reports a location, verify they match. If absent, do not check location. This allows tests to focus on message/code first and add line checks later.

---

### Q4: Target and compile path for error tests

**Context**: Error tests have `// target riscv32.q32`. `glsl_emu_riscv32_with_metadata` does full compilation (parse → semantic → codegen → ELF). Compile errors occur before ELF generation.

**Question**: Should error tests use the same compile path as run tests (`glsl_emu_riscv32_with_metadata`), or a lighter compile-only path?

**Suggested answer**: Use `glsl_emu_riscv32_with_metadata` (or equivalent). It returns `GlslError` on failure and is already used by run tests. A separate compile-only API would duplicate logic; we can refactor later if needed.

---

### Q5: Behavior when expectations are incomplete

**Context**: A file may have `// test error` but no `EXPECT_ERROR` or `EXPECT_ERROR_CODE`.

**Question**: What should happen when the test expects an error but no expectations are specified?

**Suggested answer**: Require at least one of `EXPECT_ERROR` or `EXPECT_ERROR_CODE`. If neither is present, fail the test with a clear message like "error test must specify EXPECT_ERROR and/or EXPECT_ERROR_CODE". This avoids accidentally passing tests that do not verify anything.
