## Phase 4: Filetest Validation

### Scope

Run filetests with the fastalloc path, identify which tests pass
(straight-line) vs fail (control flow), document the results. Compare
instruction counts vs adapter path and vs cranelift.

### Validation Steps

**Step 1: Baseline with adapter path**

Ensure `USE_FASTALLOC = false` in `config.rs`:

```rust
pub const USE_FASTALLOC: bool = false;
```

Run filetests to establish baseline:

```bash
scripts/glsl-filetests.sh --target rv32lp lpvm/native 2>&1 | tee /tmp/adapter_baseline.txt
```

All 33 tests should pass.

**Step 2: Fastalloc path**

Set `USE_FASTALLOC = true` in `config.rs`:

```rust
pub const USE_FASTALLOC: bool = true;
```

Run filetests:

```bash
scripts/glsl-filetests.sh --target rv32lp lpvm/native 2>&1 | tee /tmp/fastalloc_results.txt
```

**Expected results:**
- Straight-line tests (no control flow): **pass**
- Control flow tests (Label, Br, BrIf): **fail with "Unimplemented" error**

**Identify which tests have control flow:**

```bash
grep -l "Label\|Br\|BrIf" lpvm/native/*.glsl lpvm/native/**/*.glsl 2>/dev/null || echo "check manually"
```

**Step 3: Compare instruction counts**

For straight-line tests that pass with both paths, compare instruction
counts:

```bash
# Extract instruction counts from both runs
grep "inst)" /tmp/adapter_baseline.txt > /tmp/adapter_counts.txt
grep "inst)" /tmp/fastalloc_results.txt > /tmp/fastalloc_counts.txt

# Compare side by side
diff /tmp/adapter_counts.txt /tmp/fastalloc_counts.txt || true
```

**Step 4: Document results**

Create a summary in the notes:

```markdown
## M2 Validation Results

### Tests passing with fastalloc (straight-line)

- lpvm/native/native-call-simple.glsl (50 inst)
- lpvm/native/native-call-multi-args.glsl (184 inst)
- ... etc

### Tests failing (control flow - needs M3)

- lpvm/native/native-call-control-flow.glsl (has Label, BrIf)
- ... etc

### Instruction count comparison

| Test | Adapter | Fastalloc | Delta |
|------|---------|-----------|-------|
| caller-save-pressure | 148 | 148 | 0 |
| ... | ... | ... | ... |

### Observations

- Fastalloc produces identical instruction counts for tests without spills
- When spills occur, fastalloc may produce different (better/worse) counts
- Call clobber handling works correctly (save/restore sequences present)
```

### Tuning Opportunities

Based on results, identify if tuning is needed:

1. **Spill heuristics:** If fastalloc produces more spills than adapter,
   consider better eviction heuristics (cost-based instead of pure LRU).

2. **Register preference:** If long-lived values are assigned to caller-saved
   regs causing repeated spills, consider preferring callee-saved.

3. **IConst32 handling:** Verify rematerialization is working (no spill
   slots for IConst32 values).

### Decision: Enable or Disable by Default?

After validation, decide:

- **If most tests pass:** Consider enabling `USE_FASTALLOC` by default for
  straight-line functions, with automatic fallback to adapter for control
  flow.

- **If issues remain:** Keep `USE_FASTALLOC = false` by default, continue
  with M3 before wider enablement.

### Final Validation Commands

```bash
# All filetests
scripts/glsl-filetests.sh --target rv32lp lpvm/native

# Specific categories
scripts/glsl-filetests.sh --target rv32lp lpvm/native/perf

# Unit tests
cargo test -p lpvm-native --lib

# ESP32 build check (to ensure no_std compatibility)
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
  --profile release-esp32 --features esp32c6,server
```
