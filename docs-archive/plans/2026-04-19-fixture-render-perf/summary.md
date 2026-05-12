# Summary — fixture render perf (closed early)

### What was built

- `examples/perf/fastmath` — stable perf example with `glsl_opts`
  (`add_sub: wrapping`, `mul: wrapping`, `div: reciprocal`); the target
  for all per-phase profiles (`512455f6`).
- Phase 01: `accumulate_from_mapping` per-pixel multiply switched from
  `i64` to `u32` (operands provably fit, `debug_assert!` guards added).
  Saved ~43k cycles (~0.6pp) in `FixtureRuntime::render` (`3c6bc02c`).
- In-source warning above `u8_to_q32_normalized` — do not LUT-ify;
  LLVM already constant-folds the `/255` divide on this `u8` input
  (`2af977b2`).
- In-source warning above the `FixtureRuntime::render` per-channel
  post-loop — do not collapse the chain into a per-fixture LUT keyed
  on `(brightness, gamma_correction)`; the 8 KB table regressed on
  esp32c6 (`9d6cc6d2`).
- Phase 02 retrospective + Phase 04 retrospective in `00-notes.md`.

### Reverted (kept as commit-history documentation)

- Phase 02: `u8 → Q32` normalization LUT (`029f558e` reverted by
  `66cf034a`). +43k cycle regression, exactly cancelling phase 01.
- Phase 03 + 04: `ChannelLut` module + wiring into render
  (`5908e7bd` + `d46da41e` reverted by `2fcf7aae` + `d4f16360`).
  +55k cycle regression on the per-channel post-loop.

### Decisions for future reference

#### Don't replace already-cheap RV32 arithmetic with a LUT

- **Decision:** Inline arithmetic in the fixture render path stays;
  no new lookup tables of any size beyond the existing 256-byte
  `apply_gamma` table.
- **Why:** Two phases tried this on this codebase (phase 02 = 256-entry
  for `u8 → Q32`, phase 04 = 4096-entry for the per-channel transform).
  Both regressed by tens of thousands of cycles on the esp32c6 cycle
  model. The arithmetic these LUTs replaced was either constant-folded
  by LLVM (phase 02) or genuinely cheap RV32 ops with one small hot LUT
  already in the chain (phase 04). Memory loads on this target are not
  free, and tables larger than ~1 cache page evict each other across
  channel strides.
- **Rejected alternatives:** 8-bit / 256-entry per-channel LUT (would
  alias multiple Q32 accumulator values to the same output byte and
  visibly regress output); intermediate sizes (no win — same cache
  pressure, no precision benefit over inline ops).
- **Revisit when:** the target CPU changes substantially (e.g. ESP32-P4
  with cache hierarchy), or if the channel count grows enough that
  per-channel arithmetic becomes a measurable hotspot relative to the
  shader. Re-measure before assuming.

#### Profile-symbol attribution can lie — trace callers first

- **Decision:** Before optimizing a function called from a libcompiler
  helper (`__divdi3`, `u64_div_rem`, `mulhu`, etc.), confirm which
  source-level callsite is responsible. Don't trust the top-N
  symbol's name to identify the caller.
- **Why:** Phase 02 was launched on the assumption that `__divdi3` /
  `u64_div_rem` were called from `u8_to_q32_normalized`. They weren't —
  they came from JIT shader math builtins (`__lps_*_q32`,
  `__lp_lpfn_*`) and naga's compile-time evaluator. The "fix" landed
  with no reduction in either helper's self-cycles.
- **Rejected alternatives:** assuming the obvious source-level divide
  is the only one (it never is on a binary that links a JIT runtime).
- **Revisit when:** never — this is general perf hygiene.

#### Easy wins in `FixtureRuntime::render` are exhausted

- **Decision:** Stop micro-optimizing this loop. Future fixture-render
  perf work should target the higher-level pipeline
  (`shader → texture → accumulate_from_mapping → render`), not the
  per-channel post-loop.
- **Why:** After phase 01 the per-channel chain on RV32 is one Q32
  multiply, a saturating cast, and one 256-byte LUT load — there's no
  obvious ops to remove and LUT collapse regresses. Meanwhile
  `FixtureRuntime::render` inclusive cycles are 97.6% of total because
  they include `ensure_texture_rendered` / `ShaderRuntime::render` /
  `__lp_lpfn_psrdnoise2_q32` (36% self) — the shader is the dominant
  cost and lives outside this loop.
- **Rejected alternatives:** more LUT shapes (already shown to lose);
  hand-vectorizing the channel loop (RV32 esp32c6 has no useful SIMD).
- **Revisit when:** the architectural pipeline is rethought — e.g.
  fewer mapping samples per channel, batched accumulation across
  channels, shader output written directly into the channel
  accumulator format, or texture sampling moved into the mapping
  precompute.
