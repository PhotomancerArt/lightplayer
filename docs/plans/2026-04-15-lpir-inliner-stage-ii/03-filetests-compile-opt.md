# Phase 3 — Filetests `compile-opt`

## Scope of phase

- Add **`parse_compile_opt.rs`** (or equivalent) recognizing lines of the form **`// compile-opt(key, value)`** after trim: balanced parens or simple rule — **key** and **value** are trimmed strings inside **`(` `)`**, split on the **first comma** (value may contain commas if we document otherwise; MVP: no commas in **value** or use last-comma split — align with roadmap “two-part” mental model).
- In **`parse_test_file`**: handle **`compile-opt`** **before** the branch that treats lines as **`// @…`** target annotations, so **`compile-opt`** is never pushed to **`pending_annotations`**.
- Add **`config_overrides: Vec<(String, String)>`** to **`TestFile`**; on duplicate **key**, return **`Err`** with line number.
- **`filetest_lpvm`**: from **`TestFile`**, build **`CompilerConfig::default()`**, **`apply`** each pair (or merge after duplicate check), pass **`config`** into **`FaCompileOptions`**, Cranelift **`CompileOptions`**, and **`WasmOptions`** in **`compile_glsl`**.

Thread **`&TestFile`** or **`CompilerConfig`** through **`run_test_file` → compile`** as needed so **`compile_glsl`** receives overrides.

## Code Organization Reminders

- Parser tests live next to **`parse_compile_opt`** (unit tests) and optionally one integration test on a temp **`.glsl`** file in **`parse/mod.rs`** tests.
- **`AnnotationKind`** / **`parse_annotation.rs`** remain unchanged.

## Implementation Details

- **Whitespace:** allow **`// compile-opt( inline.mode , never )`** style trimming.
- **Errors:** unknown key from **`apply`** should surface with file context (path + line) when merging in the harness.

### Tests

- Parse single and multiple **`compile-opt`** lines.
- Duplicate key error.
- Invalid line syntax error (missing parens, empty key).
- End-to-end: optional minimal **`.glsl`** under **`filetests/`** with one **`compile-opt`** only if we want coverage without changing expectations — otherwise rely on parser + harness unit tests until M4 adds real tagged files.

## Validate

```bash
cargo test -p lps-filetests -- --test-threads=4
```
