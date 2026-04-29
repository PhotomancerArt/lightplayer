# Phase 1: Create Missing Filetests and Baseline Snapshots

## Scope

Establish regression safety net BEFORE changing any noise implementations.

**In scope:**
- Create filetests for worley2/3, srandom, random functions
- Add snapshot tests for ALL noise functions (not just psrdnoise2)
- Capture baseline profile

**Out of scope:**
- Any optimization changes
- Modifying existing noise function implementations

## Files to Create/Modify

### New filetests
- `lp-shader/lps-filetests/filetests/lpfn/lp_worley.glsl`
- `lp-shader/lps-filetests/filetests/lpfn/lp_random.glsl`

### Update snapshot tests
- `lp-shader/lps-builtins/tests/lpfn_q32_snapshots.rs` - Add all noise functions
- `lp-shader/lps-builtins/tests/snapshots/lpfn_q32/*.snap.txt` - Generate for all

## Implementation Details

### Filetest: lp_worley.glsl

Pattern after `lp_psrdnoise.glsl`. Test:
- Basic worley2/3 calls
- Range verification (should be [0, 1] for worley, approx [-1, 1] for worley_value)
- Determinism (same seed = same output)
- Distance metrics (Euclidean vs Manhattan)

```glsl
// @results { jit.q32 = { passed }, rv32c.q32 = { passed } }

// Basic worley2
float d2 = lpfn_worley2(vec2(5.0, 3.0), 123u);
assert(d2 >= 0.0 && d2 <= 1.5);  // Loose bounds

// Basic worley3
float d3 = lpfn_worley3(vec3(1.0, 2.0, 3.0), 456u);
assert(d3 >= 0.0 && d3 <= 1.5);

// Determinism
float d2a = lpfn_worley2(vec2(1.0, 1.0), 999u);
float d2b = lpfn_worley2(vec2(1.0, 1.0), 999u);
assert(abs(d2a - d2b) < 0.001);
```

### Filetest: lp_srandom.glsl

```glsl
// @results { jit.q32 = { passed }, rv32c.q32 = { passed } }

// Basic srandom2
float r2 = lpfn_srandom2(vec2(1.0, 2.0), 123u);
assert(r2 >= -1.0 && r2 <= 1.0);

// Basic srandom3
float r3 = lpfn_srandom3(vec3(1.0, 2.0, 3.0), 456u);
assert(r3 >= -1.0 && r3 <= 1.0);

// Determinism
float a = lpfn_srandom2(vec2(1.0, 1.0), 999u);
float b = lpfn_srandom2(vec2(1.0, 1.0), 999u);
assert(abs(a - b) < 0.0001);
```

### Snapshot Test Framework

Each noise function gets deterministic probe grid (~32 probes):

```rust
// In lpfn_q32_snapshots.rs
fn test_psrdnoise3_q32() {
    let mut output = String::new();
    for x in [-65536, 0, 65536, 131072] {
        for y in [-65536, 0, 65536, 131072] {
            for px in [0, 655360] {
                for py in [0, 655360] {
                    let (noise, gx, gy) = call_psrdnoise3_q32(x, y, 0, px, py, 0, 0);
                    output.push_str(&format!(
                        "psrdnoise3(x={x},y={y},z=0,px={px},py={py},pz=0,a=0,seed=0) = noise={noise} grad=({gx},{gy},gz)\n"
                    ));
                }
            }
        }
    }
    assert_snapshot("psrdnoise3_q32", &output);
}
```

Noise functions to snapshot:
1. psrdnoise2_q32 (already exists, verify)
2. psrdnoise3_q32
3. snoise2_q32
4. snoise3_q32
5. gnoise2_q32
6. gnoise3_q32
7. worley2_q32
8. worley3_q32
9. random2_q32
10. random3_q32

## Validate

```bash
# 1. Generate snapshots
cd /Users/yona/dev/photomancer/feature/lightplayer-emu-perf-psrdnoise2-q32
LP_UPDATE_SNAPSHOTS=1 cargo test -p lps-builtins --test lpfn_q32_snapshots

# 2. Verify snapshots pass
cargo test -p lps-builtins --test lpfn_q32_snapshots

# 3. Run filetests
scripts/filetests.sh --target jit.q32 lp_psrdnoise lp_simplex lp_gnoise lp_fbm
scripts/filetests.sh --target jit.q32 lp_worley lp_random  # new tests

# 4. Baseline profile
cargo run -p lp-cli --release -- profile examples/perf/fastmath --note p0-baseline

# 5. CI gate
just check
```

## Definition of Done

- [ ] `lp_worley.glsl` filetest created and passes
- [ ] `lp_random.glsl` filetest created and passes
- [ ] All 10 noise functions have `.snap.txt` files
- [ ] Snapshot tests pass without `LP_UPDATE_SNAPSHOTS`
- [ ] Baseline profile captured at `profiles/<ts>--examples-perf-fastmath--steady-render--p0-baseline/`
- [ ] `just check` clean
