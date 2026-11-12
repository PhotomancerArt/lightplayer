# Phase 8: Cleanup & Validation

## Scope of Phase

Final cleanup, remove any temporary code, fix warnings, and ensure everything is ready for commit.

## Code Organization Reminders

- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### Step 1: Search for temporary code

Grep for temporary code patterns:

```bash
cd lp-core
grep -r "TODO.*serde\|FIXME.*serde\|XXX.*serde" --include="*.rs"
grep -r "dbg!\|println!" --include="*.rs" | grep -i json
```

Remove any temporary debug code or TODOs related to the migration.

### Step 2: Fix warnings

Run cargo check and fix any warnings:

```bash
cd lp-core
cargo check --workspace 2>&1 | grep warning
```

Common issues to fix:
- Unused imports
- Unused variables
- Dead code

### Step 3: Format code

Run rustfmt on all changed files:

```bash
cd lp-core
cargo fmt --all
```

### Step 4: Run full test suite

Run all tests to ensure everything works:

```bash
cd lp-core
cargo test --workspace
```

### Step 5: Verify no serde_json references

Double-check that no `serde_json` imports remain in lp-core:

```bash
cd lp-core
grep -r "serde_json::" --include="*.rs" | grep -v "//\|test"
```

Should return no results (or only comments/test code).

### Step 6: Check for any remaining issues

Review the migration:
- All crates compile ✓
- All tests pass ✓
- No serde_json dependencies ✓
- No warnings ✓
- Code formatted ✓

## Validate

Run the following commands to validate:

```bash
cd lp-core
cargo check --workspace
cargo test --workspace
cargo fmt --check --all
```

Expected results:
- All crates compile without errors or warnings
- All tests pass
- Code is properly formatted
- No `serde_json` references remain (except in comments if needed)
