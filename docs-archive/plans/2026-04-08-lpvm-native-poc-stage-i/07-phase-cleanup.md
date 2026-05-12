## Phase 7: Cleanup & Validation

### Scope

Review all TODOs, fix warnings, ensure formatting, prepare for commit.

### Checklist

- [ ] Grep for `TODO`, `FIXME`, `XXX` — verify all are marked with phase numbers or M2/M3 references
- [ ] Grep for `panic!`, `unimplemented!`, `todo!` — verify all have descriptive messages
- [ ] Remove any `println!` debug output from tests
- [ ] Fix all `cargo clippy` warnings (allow `dead_code` for skeleton code)
- [ ] Run `cargo +nightly fmt -p lpvm-native`
- [ ] Verify all files have module doc comments

### Validation

```bash
# Check warnings clean
cargo clippy -p lpvm-native -- -D warnings 2>&1 | head -30

# Format
cargo +nightly fmt -p lpvm-native

# Tests pass
cargo test -p lpvm-native --lib

# Documentation builds
cargo doc -p lpvm-native --no-deps
```

### Summary update

Add to `summary.md`:
- What was completed
- What is intentionally deferred to M2/M3
- Lines of code estimate vs actual
- Any issues encountered
