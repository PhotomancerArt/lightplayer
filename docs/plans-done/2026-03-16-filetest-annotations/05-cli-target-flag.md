# Phase 5: CLI --target Flag

## Scope

Add `--target <name>` flag to `lp-glsl-filetests-app` CLI. Thread the
target filter through `lib.rs::run()` to the runner. Update
`scripts/glsl-filetests.sh` help text.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### Update `lp-glsl-filetests-app/src/main.rs`

Add `--target` option to `TestOptions`:

```rust
#[derive(Parser)]
struct TestOptions {
    /// Specify input files or directories to test (default: all tests)
    files: Vec<String>,
    /// Automatically remove annotations from tests that now pass
    #[arg(long)]
    fix: bool,
    /// Run only the specified target (e.g., "cranelift.q32", "wasm.q32")
    #[arg(long)]
    target: Option<String>,
}
```

Resolve the target name to a `&Target` using `Target::from_name()`. Pass it
through to `lp_glsl_filetests::run()`.

If the target name is invalid, print the error (which includes valid names)
and exit.

### Update `lp_glsl_filetests::run()` signature

```rust
pub fn run(files: &[String], fix_xfail: bool, target_filter: Option<&Target>) -> Result<()>
```

Inside `run()`:

```rust
let active_targets: Vec<&Target> = if let Some(t) = target_filter {
    vec![t]
} else {
    DEFAULT_TARGETS.iter().collect()
};
```

Pass `active_targets` to `run_filetest_with_line_filter`.

### Update `run_filetest_with_line_filter`

Accept `targets: &[&Target]` and pass through to the runner.

### Update `scripts/glsl-filetests.sh`

Pass through `--target` flag if provided. Update help text to mention the
flag and list valid targets.

### Tests

- Manual testing: `cargo run -p lp-glsl-filetests-app -- test --target wasm.q32`
  should run only wasm tests
- Manual testing: `cargo run -p lp-glsl-filetests-app -- test --target invalid`
  should print valid target names and exit with error

## Validate

```
cargo build -p lp-glsl-filetests-app
cargo test -p lp-glsl-filetests
cargo +nightly fmt -- --check
```
