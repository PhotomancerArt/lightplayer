## Phase 4: Testing and Tuning

### Scope
Validate correctness and measure performance improvement. Compare instruction counts vs greedy allocator baseline.

### Validation Commands

#### Correctness
```bash
# Full filetest suite
cargo run -p lps-filetests-app -- test -t rv32lp.q32

# Specific perf tests
cargo run -p lps-filetests-app -- test lpvm/native/perf/ -t rv32lp.q32
```

#### Performance Baseline
Run with greedy allocator first (revert temporarily), record instruction counts:
```bash
# mat4-reg-pressure: expect ~2000 inst with greedy
cargo run -p lps-filetests-app -- test lpvm/native/perf/mat4-reg-pressure.glsl -t rv32lp.q32 --detail
```

Then run with linear scan and compare:
```bash
# mat4-reg-pressure: expect lower count with linear scan
cargo run -p lps-filetests-app -- test lpvm/native/perf/mat4-reg-pressure.glsl -t rv32lp.q32 --detail
```

### Target Improvements

| Test | Greedy | Linear Scan | Target |
|------|--------|-------------|--------|
| mat4-reg-pressure.glsl | ~2000 | TBD | < 1500 |
| spill-density.glsl | ~1000 | TBD | < 800 |
| caller-save-pressure.glsl | ~250 | TBD | < 220 |

### Debugging
If tests fail:
1. Check which vregs are spilled vs greedy
2. Verify interval boundaries are correct
3. Check that caller-saved handling matches

### Tuning
If performance not improved:
1. Review spill heuristic
2. Check interval ordering
3. Consider register preference (callee-saved first)
