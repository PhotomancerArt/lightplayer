# Phase 5: Delete Legacy lps-exec Crate

## Scope

Delete the `lp-shader/legacy/lps-exec/` crate containing the `GlslExecutable` trait.
This was the original uniform interface for running compiled GLSL from filetests,
now superseded by the `LpvmEngine`/`LpvmModule`/`LpvmInstance` traits.

## Verification Before Deletion

### Check for Consumers

```bash
# Check if anything still imports from lps-exec
rg "lps_exec|GlslExecutable" lp-shader/ --glob "*.rs"

# Check workspace Cargo.toml for dependency
rg "lps-exec" Cargo.toml lp-shader/*/Cargo.toml
```

**Should find:** Only references within the legacy directory itself, or no references.

### Check lps-diagnostics

The crate comment mentions lps-exec:
```rust
//! Stack: `lps-diagnostics` → `lps-shared` → `lpvm` → `lps-exec`.
```

This is likely just documentation. Verify it's not an actual dependency.

## Files to Delete

```
lp-shader/legacy/lps-exec/
├── Cargo.toml
└── src/
    ├── lib.rs
    └── executable.rs
```

## Files That May Reference lps-exec (update these)

| File | Action |
|------|--------|
| `lp-shader/lps-diagnostics/src/lib.rs` | Update comment if it mentions lps-exec |
| `lp-shader/lps-shared/src/lib.rs` | Update comment: "Used by lps-exec, lpvm, and lps-filetests" |

## Code Organization Reminders

- Verify NO workspace members depend on lps-exec
- Check `Cargo.toml` workspace members list
- Update any documentation that mentions lps-exec

## Validate

```bash
# After deletion, workspace should still compile
cargo check --workspace --lib 2>&1 | head -50

# No broken references
cargo check -p lps-filetests --lib
cargo check -p lps-diagnostics --lib
```

## Phase Notes

- `GlslExecutable` was the old trait that `lps-filetests` used
- New filetests use `LpvmEngine` trait implementations directly
- If any references remain, add them to the migration list before deleting
