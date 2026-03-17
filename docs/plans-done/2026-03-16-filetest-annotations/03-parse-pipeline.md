# Phase 3: Update TestFile and Parse Pipeline

## Scope

Integrate the annotation parser into the main `parse_test_file` pipeline.
Update `TestFile` and `RunDirective` to carry annotations instead of
`expect_fail` and `target`. Remove `[expect-fail]` parsing. Make `// target`
optional.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### Update `src/parse/test_type.rs`

Update `TestFile`:

```rust
pub struct TestFile {
    pub test_types: Vec<TestType>,
    pub glsl_source: String,
    /// File-level annotations (between // test and first GLSL code).
    pub annotations: Vec<Annotation>,
    pub run_directives: Vec<RunDirective>,
    pub error_expectations: Vec<ErrorExpectation>,
    pub trap_expectations: Vec<TrapExpectation>,
    pub clif_expectations: ClifExpectations,
}
```

Remove:
- `target: Option<String>` field — no longer needed
- `is_test_run: bool` field — derive from `test_types`

Update `RunDirective`:

```rust
pub struct RunDirective {
    pub expression_str: String,
    pub comparison: ComparisonOp,
    pub expected_str: String,
    pub tolerance: Option<f32>,
    pub line_number: usize,
    /// Annotations attached to this directive.
    pub annotations: Vec<Annotation>,
}
```

Remove:
- `expect_fail: bool` field — replaced by annotations

### Update `src/parse/parse_run.rs`

Remove `[expect-fail]` parsing from `parse_run_directive`. The function should
no longer look for or strip `[expect-fail]` suffixes. The `RunDirective` no
longer has `expect_fail`.

Update tests to remove all `[expect-fail]` test cases and add equivalents
that test the annotation attachment (in the integration flow).

### Update `src/parse/parse_target.rs`

Keep `parse_target_directive` for backward compatibility during migration, but
it's now optional. A file without `// target` is valid.

### Update `src/parse/parse_source.rs`

Ensure annotation lines (`// @...`) are stripped from the GLSL source passed
to the compiler. They're comments so the GLSL parser would ignore them, but
stripping them keeps the source clean and avoids line number confusion.

### Update `src/parse/mod.rs` — main parse pipeline

The parse loop needs to:

1. Collect file-level annotations (annotations before any GLSL code or run
   directives)
2. Collect pending directive-level annotations (annotations immediately before
   a `// run:` line)
3. Attach pending annotations to the next `RunDirective`

```rust
pub fn parse_test_file(path: &Path) -> Result<TestFile> {
    let contents = std::fs::read_to_string(path)?;
    let lines: Vec<String> = contents.lines().map(|s| s.to_string()).collect();

    let mut test_types = Vec::new();
    let mut file_annotations = Vec::new();
    let mut run_directives = Vec::new();
    let mut trap_expectations = Vec::new();
    let mut pending_annotations: Vec<Annotation> = Vec::new();
    let mut seen_glsl_code = false;

    for (line_num, line) in lines.iter().enumerate() {
        let line_number = line_num + 1;

        // Test type directives
        if let Some(test_type) = parse_test_type::parse_test_type(line) {
            test_types.push(test_type);
            continue;
        }

        // Skip legacy // target lines (ignored, no longer needed)
        if parse_target::parse_target_directive(line).is_some() {
            continue;
        }

        // Annotation lines
        if let Some(annotation) = parse_annotation::parse_annotation_line(line, line_number)? {
            if !seen_glsl_code && run_directives.is_empty() {
                // Before any GLSL code or run directives → file-level
                file_annotations.push(annotation);
            } else {
                // After GLSL code → pending directive-level
                pending_annotations.push(annotation);
            }
            continue;
        }

        // Run directives
        if let Some(run_line) = parse_run::parse_run_directive_line(line) {
            let mut directive = parse_run::parse_run_directive(run_line, line_number)?;
            directive.annotations = std::mem::take(&mut pending_annotations);
            run_directives.push(directive);
            continue;
        }

        // Trap expectations
        if let Some(trap_exp) = parse_trap::parse_trap_expectation(line, line_number)? {
            trap_expectations.push(trap_exp);
            continue;
        }

        // Any non-directive, non-blank, non-comment line = GLSL code
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("//") {
            seen_glsl_code = true;
        }
    }

    // ... error expectations, source extraction as before ...

    Ok(TestFile {
        glsl_source,
        run_directives,
        trap_expectations,
        test_types,
        clif_expectations,
        error_expectations,
        annotations: file_annotations,
    })
}
```

### Tests

In `parse/mod.rs` or a new `parse/tests.rs`:

- `test_parse_file_no_target` — file without `// target` parses successfully
- `test_parse_file_level_annotation` — `// @unimplemented(backend=wasm)` in
  header → appears in `test_file.annotations`
- `test_parse_directive_annotation` — annotation before `// run:` → attached
  to directive
- `test_parse_mixed_annotations` — file-level + directive-level both present
- `test_parse_stacked_annotations` — two annotations before one `// run:`
- `test_parse_legacy_target_ignored` — `// target riscv32.q32` doesn't cause
  errors, just gets ignored
- `test_parse_annotation_not_in_glsl` — annotation lines stripped from
  glsl_source

## Validate

```
cargo build -p lp-glsl-filetests
cargo test -p lp-glsl-filetests
cargo +nightly fmt -- --check
```

Note: the runner (test_run) will have compilation errors after this phase
because `RunDirective.expect_fail` and `TestFile.target` are removed. That's
expected — phase 4 fixes the runner. To keep things compiling, you can
temporarily add `#[allow(dead_code)]` on the new fields or update the runner
references to use temporary stubs. The cleanest approach is to do phases 3
and 4 together in one pass if preferred.
