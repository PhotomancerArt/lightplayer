# Phase 2: Annotation Parser

## Scope

Implement parsing of `@unimplemented(...)`, `@broken(...)`, and `@ignore(...)`
annotation lines into `Annotation` structs. This is a standalone parser module
with no integration into the main parse pipeline yet.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### Create `src/parse/parse_annotation.rs`

The parser recognizes lines of the form:

```
// @unimplemented()
// @unimplemented(backend=wasm)
// @broken(backend=cranelift, isa=riscv32)
// @broken(isa=riscv32, reason="overflow in emulator")
// @ignore(backend=wasm)
```

Grammar:

```
annotation_line := '//' SPACE '@' kind '(' params ')'
kind            := 'unimplemented' | 'broken' | 'ignore'
params          := ''                          (empty)
                 | param (',' param)*
param           := key '=' value
key             := 'backend' | 'float_mode' | 'isa' | 'exec_mode' | 'reason'
value           := quoted_string | identifier
quoted_string   := '"' [^"]* '"'
identifier      := [a-zA-Z0-9_]+
```

Implementation approach:

```rust
use crate::target::{
    Annotation, AnnotationKind, Backend, ExecMode, FloatMode, Isa, TargetFilter,
};
use anyhow::Result;

/// Try to parse an annotation from a comment line.
/// Returns None if the line is not an annotation.
pub fn parse_annotation_line(line: &str, line_number: usize) -> Result<Option<Annotation>> {
    let trimmed = line.trim();
    let rest = match trimmed.strip_prefix("// @") {
        Some(r) => r,
        None => return Ok(None),
    };

    // Extract kind and params
    let paren_start = rest.find('(')
        .ok_or_else(|| anyhow::anyhow!("line {}: annotation missing '('", line_number))?;
    let kind_str = &rest[..paren_start];
    let kind = parse_annotation_kind(kind_str, line_number)?;

    // Extract content between parens
    let paren_end = rest.rfind(')')
        .ok_or_else(|| anyhow::anyhow!("line {}: annotation missing ')'", line_number))?;
    let params_str = rest[paren_start + 1..paren_end].trim();

    let (filter, reason) = parse_params(params_str, line_number)?;

    Ok(Some(Annotation {
        kind,
        filter,
        reason,
        line_number,
    }))
}
```

Helper functions (bottom of file):

- `parse_annotation_kind(s, line) -> Result<AnnotationKind>` — maps string to
  enum variant
- `parse_params(s, line) -> Result<(TargetFilter, Option<String>)>` — splits
  on commas, parses key=value pairs, populates filter fields and extracts
  reason
- `parse_backend(s, line) -> Result<Backend>` — "cranelift" | "wasm"
- `parse_float_mode(s, line) -> Result<FloatMode>` — "q32" | "f32"
- `parse_isa(s, line) -> Result<Isa>` — "riscv32" | "wasm32" | "native"
- `parse_exec_mode(s, line) -> Result<ExecMode>` — "jit" | "emulator"

Error messages should include the line number and the unrecognized value.

### Tests

In `parse_annotation.rs`, `#[cfg(test)] mod tests`:

- `test_parse_empty_annotation` — `// @unimplemented()` → kind=Unimplemented,
  filter=default, reason=None
- `test_parse_backend_filter` — `// @unimplemented(backend=wasm)` →
  filter.backend=Some(Wasm)
- `test_parse_multiple_filters` — `// @broken(backend=cranelift, isa=riscv32)`
  → both fields set
- `test_parse_with_reason` — `// @broken(isa=riscv32, reason="overflow")` →
  filter.isa set, reason="overflow"
- `test_parse_ignore` — `// @ignore(backend=wasm)` → kind=Ignore
- `test_parse_all_filter_fields` — all four axes set at once
- `test_parse_not_annotation` — `// run: test() == 1` → returns None
- `test_parse_not_comment` — `int x = 5;` → returns None
- `test_parse_invalid_kind` — `// @foobar()` → error
- `test_parse_invalid_key` — `// @broken(foo=bar)` → error
- `test_parse_invalid_backend` — `// @broken(backend=gcc)` → error
- `test_parse_reason_with_quotes` — `reason="has spaces and, commas"` parsed
  correctly
- `test_parse_whitespace_tolerance` — extra spaces around `=` and `,` handled

## Validate

```
cargo build -p lp-glsl-filetests
cargo test -p lp-glsl-filetests
cargo +nightly fmt -- --check
```
