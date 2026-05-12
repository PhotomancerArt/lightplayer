# Fixture render perf — notes

## Scope of work

Three targeted micro-optimizations on the fixture render hot path, surfaced
by CPU profiling (`profiles/2026-04-20T09-59-13--examples-basic--steady-render--wrapping-reciprocal/report.txt`):

1. **u32 mul in accumulation** — replace the `i64 * i64 >> 16` per-RGB
   multiply in `accumulate_from_mapping` with a `u32 * u32 >> 16`. Range
   analysis: `norm.0 ≤ 65535` and `frac.0 ≤ 65536`, product fits in `u32`.
   Kills the i64 codegen on RV32 (currently `mul`+`mulhu`+compose).
2. **256-entry `u8 → Q32` LUT** — replace `u8_to_q32_normalized`'s
   `(v * 65536) / 255` with a `static U8_TO_Q32: [Q32; 256]` const-eval'd
   from the same formula. Kills the `__divdi3` call (currently ~2% of
   total cycles, per the profile report).
3. **Per-fixture channel LUT** — collapse the per-channel post-loop
   transform `Q32 → ×brightness → to_u16_saturating → (optional gamma) →
   u16` into a single LUT lookup keyed by the top-N bits of the saturated
   accumulator. Recomputed on `update_config` when `brightness` or
   `gamma_correction` changes.

Out of scope (deliberately deferred to a follow-up plan):

- Hoisting `ctx.get_output(...)` out of the loop (touches `RenderContext`
  trait surface and `OutputProvider` plumbing — bigger blast radius).
- Caching `lamp_colors` / accumulator `Vec`s on the runtime to avoid
  per-frame allocation.
- Devirtualizing `TextureSampler` dispatch.
- Sampler-level changes to keep 16-bit precision through accumulation.

## Current state of the codebase as it pertains to the scope

### Hot path entry: `FixtureRuntime::render`

`lp-core/lp-engine/src/nodes/fixture/runtime.rs`, lines ~228–335. The
relevant per-channel loop (lines 307–329) is:

```rust
for channel in 0..=max_channel as usize {
    let r_q = ch_values_r[channel] * brightness;
    let g_q = ch_values_g[channel] * brightness;
    let b_q = ch_values_b[channel] * brightness;

    let mut r = r_q.to_u16_saturating();
    let mut g = g_q.to_u16_saturating();
    let mut b = b_q.to_u16_saturating();

    lamp_colors[channel * 3]     = (r >> 8) as u8;
    lamp_colors[channel * 3 + 1] = (g >> 8) as u8;
    lamp_colors[channel * 3 + 2] = (b >> 8) as u8;

    if self.gamma_correction {
        r = apply_gamma((r >> 8) as u8).to_q32().to_u16_saturating();
        g = apply_gamma((g >> 8) as u8).to_q32().to_u16_saturating();
        b = apply_gamma((b >> 8) as u8).to_q32().to_u16_saturating();
    }

    let start_ch = channel_offset + (channel as u32) * 3;
    let buffer = ctx.get_output(output_handle, universe, start_ch, 3)?;
    self.color_order.write_rgb_u16(buffer, 0, r, g, b);
}
```

`brightness: u8` and `gamma_correction: bool` are stored directly on
`FixtureRuntime` and assigned in `init` and `update_config` (lines ~218,
~383).

### Hot path: `accumulate_from_mapping`

`lp-core/lp-engine/src/nodes/fixture/mapping/accumulation.rs`. The
inner-loop body for the partial-contribution case (lines 110–124):

```rust
let frac = Q32(contribution_raw);                       // [1, 65535]
let norm_r = u8_to_q32_normalized(pixel_r);             // [0, 65536]
let norm_g = u8_to_q32_normalized(pixel_g);
let norm_b = u8_to_q32_normalized(pixel_b);
let accumulated_r = Q32(((norm_r.0 as i64 * frac.0 as i64) >> 16) as i32);
let accumulated_g = Q32(((norm_g.0 as i64 * frac.0 as i64) >> 16) as i32);
let accumulated_b = Q32(((norm_b.0 as i64 * frac.0 as i64) >> 16) as i32);
```

Where:

```rust
fn u8_to_q32_normalized(v: u8) -> Q32 {
    Q32(((v as i64) * 65536 / 255) as i32)              // <-- /255 is __divdi3
}
```

### Range invariants (worth a `debug_assert!` somewhere)

- `entry.contribution_raw()` returns a `u32` masked with `0xFFFF`, so it
  is in `[0, 65535]` by construction (`mapping/entry.rs:61`). The current
  code only enters the multiply branch when `contribution_raw != 0`, so
  `frac.0 ∈ [1, 65535]`.
- After phase 2, `norm.0` is in `[0, 65535]` (LUT range), so the product
  is at most `65535 * 65535 ≈ 4.295 × 10^9`, which fits in `u32::MAX =
  4.295 × 10^9`. Just barely — worth asserting.

### Profile evidence

From the most recent profile (`profiles/2026-04-20T09-59-13--examples-basic--steady-render--wrapping-reciprocal/report.txt`):

- `__divdi3`: ~2.0% self
- `compiler_builtins::int::specialized_div_rem::u64_div_rem`: ~2.3% self
- `FixtureRuntime::render`: visible in top-N (collapsed by symbol now)
- `Rgba16Sampler::sample_pixel`: ~3.5% self

The `__divdi3` and `u64_div_rem` are almost certainly the `(v * 65536) /
255` divide in `u8_to_q32_normalized`, called 3× per non-skip mapping
entry.

### Existing test patterns

`lp-core/lp-engine/src/nodes/fixture/runtime.rs` has a `#[cfg(test)] mod
tests` at line ~434. Sibling files use the same pattern. New tests for
this plan go inline in the same module-test pattern. No new test crate
needed.

## Answered questions

| # | Question | Answer |
| --- | --- | --- |
| Q1 | Plan dir name `2026-04-19-fixture-render-perf`? | Yes |
| Q2 | Scope = the three ideas only, nothing else? | Yes |
| Q3 | Commit per phase (override default single-commit)? | Yes — small commits, observe profile delta after each |
| Q8 | Profile target example? | `examples/perf/fastmath` (stable, fast-math `glsl_opts`); the user's earlier mention of `examples/perf/baseline` was a typo |
| Q9 | Who commits per phase? | Sub-agent. Each phase file has the exact commit message body to use; main agent reviews diff first, sub-agent commits second |

Discussion-style questions (Q4–Q7) are tracked in chat and answers will
be backfilled here as they're resolved.

## Discussion-style answers

### Q4 — LUT precision: 12-bit input (`[u16; 4096]`, 8 KB per fixture)

Per-channel cost is identical to 8-bit (one `min` + one shift + one
load). The 8 KB cost is acceptable because `FixtureRuntime` already has
a `shed_optional_buffers` mechanism that the LUT will plug into
(`channel_lut: Option<ChannelLut>`).

### Q5 — Lazy rebuild + conditional invalidation

- `channel_lut: Option<ChannelLut>` field, defaults to `None`.
- `init()` does not eagerly build.
- `update_config()` invalidates (`= None`) only when `brightness` or
  `gamma_correction` actually changed — cheap equality check.
- `shed_optional_buffers()` also sets `channel_lut = None`.
- `render()` does
  `self.channel_lut.get_or_insert_with(|| ChannelLut::build(self.brightness, self.gamma_correction))`.

### Q6 — New file `nodes/fixture/channel_lut.rs`

Sibling files in this directory (`gamma.rs`, `entry.rs`, `accumulation.rs`,
`points.rs`) all follow one-concept-per-file. `runtime.rs` is already
~947 lines.

### Q7 — Reference function + exhaustive sweep

- Slow-path reference function `channel_transform_reference(ch_q32,
  brightness, gamma) -> u16` lives in `channel_lut.rs`.
- `ChannelLut::build` *uses* this reference function to populate the
  table — eliminates drift between two parallel implementations.
- Test `build_matches_reference_exhaustive`: for `brightness ∈ {0, 1, 8,
  32, 64, 127, 200, 255}` × `gamma ∈ {false, true}`, walk every one of
  the 4096 input bins and assert `lut.lookup(bin_q32) ==
  channel_transform_reference(bin_q32, brightness, gamma)`. ~64k
  assertions, microseconds.
- Saturation test: `lookup(Q32(ONE * 2))` matches `lookup(Q32(ONE - 1))`.
- Boundary tests: `brightness=0` → all zeros.
- Optional integration test in `runtime.rs` tests asserting end-to-end
  `lamp_colors` equality after a render, as paranoia at the boundary.

## Phase outline

| # | Title | Sub-agent | Profile after? | Status |
| --- | --- | --- | --- | --- |
| 00 | Configure fastmath example + capture baseline profile | main | yes (`p0-baseline`) | done (`512455f6`) |
| 01 | Accumulation: u32 multiply | yes | yes (`p1-u32mul`) | done (`3c6bc02c`) — saved 43k cycles |
| 02 | Accumulation: u8→Q32 LUT | yes | yes (`p2-u8lut`) | **reverted** — see "Phase 02 retrospective" below |
| 03 | ChannelLut module (new file, no integration) | yes | no (no behaviour change) | **reverted** — see "Phase 04 retrospective" below |
| 04 | Wire ChannelLut into FixtureRuntime | yes | yes (`p4-channel-lut`) | **reverted** — see "Phase 04 retrospective" below |
| 05 | Cleanup, validation, summary profile | supervised | yes (`p5-final`) | skipped — plan closed early after phase 04 regression |

All phases run sequentially. No parallel groups (each phase depends on
the previous one being merged for accurate per-phase profile attribution).

## Phase 02 retrospective

**Commit:** `029f558e` (reverted by `66cf034a`).

**What we predicted:** `__divdi3` (~2.1% self) and `u64_div_rem` (~2.7%
self) were called from `u8_to_q32_normalized`'s `(v * 65536) / 255`
divide. Replacing the divide with a 256-entry LUT should kill those
helpers.

**What actually happened:** post-LUT profile showed `__divdi3` and
`u64_div_rem` self-cycles **byte-identical** to the pre-LUT profile
(163,500 / 217,686 cycles, unchanged). Meanwhile
`FixtureRuntime::render` self regressed by +43k cycles (back to p0
baseline). Net: −0.5pp regression.

**Root cause:** the `__divdi3` / `u64_div_rem` helpers in this binary
are called from JIT shader math builtins (`__lps_cosh_q32`,
`__lps_acos_q32`, `__lp_lpfn_psrdnoise2_q32`, ...) and from naga's
compile-time constant evaluator + `pp_rs` preprocessor. None of those
callers are reached from the engine-side `accumulate_from_mapping` path.

The original `(v * 65536) / 255` divide was almost certainly being
constant-folded by LLVM into a magic-multiply sequence at compile time
(255 is a constant divisor, the input is a `u8`, so the divide reduces
to `(v * magic) >> shift` — a few in-register instructions). The LUT
replaced that fast inline arithmetic with a memory load + (probable)
bounds check, which costs more on RV32 than the magic multiply.

**Lessons for future plans:**

1. **Symbol-level profile attribution can lie.** A `__divdi3` line in
   the top-N tells you a divide helper is hot, but does NOT tell you
   *which* divide. Always trace callers (e.g. `riscv32-elf-objdump -d`
   + grep for callsites of the helper) before assuming a specific
   source-level expression is responsible.
2. **Constant-foldable arithmetic may already be cheaper than a LUT.**
   When a divide has a compile-time constant divisor, LLVM lowers it
   to `mul + shift` already — replacing it with a memory load can be
   a regression on cache-warm short tables.
3. **Per-phase profiling caught this in one commit.** That's the whole
   point of the commit-per-phase strategy.

Phase 03 and Phase 04 are independent of Phase 02 and proceed unchanged.

## Phase 04 retrospective

**Commits:** `5908e7bd` (phase 03 — ChannelLut module) + `d46da41e`
(phase 04 — wire into `FixtureRuntime::render`). Reverted by `2fcf7aae`
+ `d4f16360`. In-source warning added at the per-channel loop in
`lp-core/lp-engine/src/nodes/fixture/runtime.rs` so future readers don't
relitigate this.

**What we predicted:** the per-channel post-loop ran a Q32 multiply
(brightness), a saturating cast, and a conditional gamma byte-LUT lookup
per RGB channel. Collapsing that chain into a single
`(brightness, gamma) → u16` lookup keyed on a 12-bit input bin (4096 ×
u16 = 8 KB) should be a clean win — replace ~5 ops + 1 small load with
1 larger load.

**What actually happened:** post-LUT profile (`p4-channel-lut`)
regressed `FixtureRuntime::render` self-cycles by **+55,244 (+0.6pp)**
vs the `p0-baseline` measured at the start of the plan. Total
attributed cycles also rose by ~54k. Every other top-20 entry was
bit-identical; the delta is entirely in the per-channel post-loop.

| metric | p0-baseline | p4-channel-lut | delta |
| --- | --- | --- | --- |
| `FixtureRuntime::render` self | 1,176,148 (14.8%) | 1,231,392 (15.4%) | **+55,244 (+0.6pp)** |
| total attributed cycles | 7,948,854 | 8,002,411 | +53,557 |

**Root cause (best read):** the pre-phase-04 chain was already cheap on
RV32 — a Q32 multiply (`mulhu` + `mul`), a saturating cast (compare +
shift), and a conditional 256-byte gamma LUT load that stays hot. The
8 KB replacement table spans ~256 cache lines on the esp32c6 cycle
model. Each channel stride touches a fresh line, and the load is not
cheaper than the ops it replaces. The 256-byte gamma LUT wins precisely
because it's small enough to stay resident.

**Lessons (extending phase 02's):**

1. **"LUT > arithmetic" is not free on this CPU model.** Two phases in
   a row, the same intuition lost: replacing already-cheap inline
   arithmetic with a memory load regressed. The gamma LUT works because
   it's 256 bytes; a 8 KB LUT spans too many lines to amortize.
2. **Smaller LUTs alias output.** An 8-bit (256-entry) input LUT was
   the next-smaller option but would alias multiple Q32 accumulator
   values to the same byte and regress visible output. There's no
   middle ground here that's both correct and cache-friendly.
3. **The pre-existing path was already well-tuned.** `apply_gamma` over
   a 256-byte LUT + Q32 multiply is genuinely the right shape on RV32.
   Don't fight it.
4. **Phase 01 (u32 multiply, `3c6bc02c`) is the only real win** from
   this plan — saved ~43k cycles in `FixtureRuntime::render` and is
   landed. Plan closed early.

## Per-phase commit/profile flow

## Per-phase commit/profile flow

Each phase that builds (i.e. all of them) follows this exact sequence:

1. Sub-agent makes the code change.
2. Sub-agent runs the phase's validation command(s).
3. Main agent reviews the diff (`git diff`) and re-runs validation.
4. **Sub-agent commits** using the heredoc message in the phase file.
   Conventional Commits: `<type>(lp-engine): <description>` plus a
   `Plan: docs/plans/2026-04-19-fixture-render-perf/NN-<slug>.md` line.
5. Sub-agent runs the profile command (where applicable):
   ```bash
   cargo run -p lp-cli --release -- profile examples/perf/fastmath --note <pN-shortname>
   ```
6. Sub-agent finds the resulting profile dir
   (`ls -dt profiles/*--<pN-shortname> | head -n 1`) and reports back
   the path + the top 10 entries of its `report.txt`.

`profiles/` is gitignored — profile runs never appear in git status.

## Finalization (after phase 05)

The `/implement` command's standard "single commit + move to plans-old"
finalization does **not** apply because we commit per phase. Instead,
after phase 05 lands the main agent does one wrap-up commit:

1. Write `summary.md` (per `/implement` template).
2. `git mv docs/plans/2026-04-19-fixture-render-perf docs/plans-old/2026-04-19-fixture-render-perf`.
3. Single commit:
   `chore(docs): archive fixture-render-perf plan` with body listing
   per-phase commit SHAs and a reference to the captured profile dirs.
