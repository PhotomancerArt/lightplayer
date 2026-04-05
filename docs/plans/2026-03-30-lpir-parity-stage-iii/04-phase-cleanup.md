# Phase 4: Cleanup & Validation

## Scope of phase

Final cleanup, remove any temporary code, ensure all tests pass, and prepare for commit.

## Cleanup tasks

### Remove debug/temporary code

```bash
# Search for TODOs, FIXMEs, debug prints
cd /Users/yona/dev/photomancer/lp2025
grep -r "TODO\|FIXME\|println!\|dbg!" lp-shader/lps-frontend/src/ --include="*.rs" | grep -v "test"
grep -r "TODO\|FIXME\|println!\|dbg!" lp-shader/legacy/lpir-cranelift/src/ --include="*.rs" | grep -v "test"
```

Remove any temporary debug code added during development.

### Fix warnings

```bash
# Check for warnings
cargo +nightly fmt --check -p lps-frontend
cargo clippy -p lps-frontend -- -D warnings
cargo clippy -p lpir-cranelift -- -D warnings
```

### Final validation

Run the complete validation suite:

```bash
cd /Users/yona/dev/photomancer/lp2025

# 1. Format check
cargo +nightly fmt -p lps-frontend -p lpir-cranelift

# 2. Unit tests
cargo test -p lps-frontend --no-fail-fast
cargo test -p lpir-cranelift --no-fail-fast

# 3. Embedded target check
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server

# 4. Filetests - the main validation
./scripts/glsl-filetests.sh --target jit.q32

# 5. Check specific areas touched by this plan
./scripts/glsl-filetests.sh --target jit.q32 "vec/bvec2/" "vec/bvec3/" "vec/bvec4/"
./scripts/glsl-filetests.sh --target jit.q32 "const/builtin/extended.glsl"
./scripts/glsl-filetests.sh --target jit.q32 "builtins/common-round.glsl" "builtins/common-roundeven.glsl"
```

## Plan cleanup

### Update summary.md

Create `docs/plans/2026-03-30-lpir-parity-stage-iii/summary.md`:

```markdown
# LPIR Parity Stage III Summary

## Completed work

1. **Bvec casts** - Fixed `As` and `Compose` lowering for bvec to numeric vector conversions
2. **Q32 round** - Promoted to implemented, updated spec, removed test annotations
3. **Test triage** - Rewrote non-standard syntax tests, annotated genuine Naga limitations

## Files changed

- `lp-shader/lps-frontend/src/lower_expr.rs`
- `docs/design/q32.md`
- `lp-shader/lps-filetests/filetests/const/builtin/extended.glsl`
- (other test files as needed)

## Validation results

- jit.q32: X/Y tests passing (update with actual numbers)
- Zero unexpected failures
```

### Move plan to done

After commit, move plan directory:

```bash
mv docs/plans/2026-03-30-lpir-parity-stage-iii docs/plans-done/
```

## Commit

Once all validation passes:

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat(lps-frontend): LPIR parity stage III - bvec casts, round, test triage

- Fix bvec to numeric vector cast lowering (As, Compose)
- Promote Q32 round to implemented (half-away-from-zero semantics)
- Update Q32 spec §5 to reflect round as implemented
- Triage Naga-limited tests: rewrite non-standard syntax, annotate genuine limitations
- Remove @unimplemented from const/builtin/extended.glsl (round now works)
EOF
)"
```

## Final check

```bash
# Ensure no stray files
git status

# Quick sanity test
./scripts/glsl-filetests.sh --target jit.q32 | tail -5
```
