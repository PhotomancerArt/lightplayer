# Phase 8: Update file_update.rs for New Format

## Scope

Update `FileUpdate` to work with the new annotation format. Replace
`add_expect_fail_marker` and `remove_expect_fail_marker` with annotation-aware
equivalents. Update `LP_FIX_XFAIL` and `LP_MARK_FAILING_TESTS_EXPECTED`
behavior in `lib.rs`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### Update `src/util/file_update.rs`

Replace:

```rust
pub fn add_expect_fail_marker(&self, line_number: usize) -> Result<()>
pub fn remove_expect_fail_marker(&self, line_number: usize) -> Result<()>
```

With:

```rust
/// Add an `@unimplemented()` annotation line before the run directive at
/// the given line number.
pub fn add_annotation(&self, line_number: usize, annotation: &str) -> Result<()>

/// Remove an annotation line immediately before the run directive at the
/// given line number.
pub fn remove_annotation(&self, line_number: usize) -> Result<()>
```

`add_annotation` inserts `// @unimplemented()` (or the given annotation
string) on the line before the run directive. It adjusts `line_diff` to
account for the inserted line.

`remove_annotation` looks for an annotation line (`// @...`) immediately
before the run directive and removes it. If there are stacked annotations,
it removes the first one that matches (or all of them — depending on the
use case). For `--fix` mode (unexpected pass), removing all annotations
for this directive is correct since the test now passes everywhere.

### Update `src/lib.rs` — fix/bless behavior

Update the `fix_xfail` code path:

- When a test has an unexpected pass, call `file_update.remove_annotation`
  instead of `file_update.remove_expect_fail_marker`
- Update the display messages: "expect-fail" → "annotation"

Update the `mark_failing_expected` code path:

- When marking failing tests, call `file_update.add_annotation` instead of
  `file_update.add_expect_fail_marker`
- The annotation to add is `// @unimplemented()` (no filter — broken
  everywhere)
- Update the display messages accordingly

Consider renaming environment variables:

- `LP_FIX_XFAIL` → keep for now (or rename to `LP_FIX_ANNOTATIONS`)
- `LP_MARK_FAILING_TESTS_EXPECTED` → keep for now

### Update display strings

Throughout `lib.rs`, update user-facing messages:

- "expect-fail" → "expected-failure" or "annotation"
- "[expect-fail]" → "@unimplemented()" in marker-related messages
- "unexpected-pass" stays the same (it's accurate)

### Tests

- `test_add_annotation` — inserts `// @unimplemented()` before run directive
- `test_remove_annotation` — removes annotation line before run directive
- `test_remove_stacked_annotations` — removes all annotation lines
- `test_add_annotation_line_tracking` — line_diff updated correctly after
  insertion

## Validate

```
cargo build -p lps-filetests
cargo test -p lps-filetests
cargo +nightly fmt -- --check
```

Manual test:

```
# Mark a test as expected-failure, then fix it
LP_MARK_FAILING_TESTS_EXPECTED=1 scripts/filetests.sh scalar/int/op-add.glsl
# Verify @unimplemented() was added
# Then remove it:
scripts/filetests.sh --fix scalar/int/op-add.glsl
```
