# Phase 04 — Wire ChannelLut into FixtureRuntime

**Sub-agent:** yes (Composer 2)
**Parallel:** —
**Profile after:** yes — `p4-channel-lut`

## Scope of phase

Plug the `ChannelLut` from phase 03 into `FixtureRuntime`:

1. Add a `channel_lut: Option<ChannelLut>` field, defaulting to `None`.
2. Lazy-build the LUT on first `render()` via `get_or_insert_with`.
3. Conditionally invalidate the LUT in `update_config` when (and only
   when) `brightness` or `gamma_correction` actually changed.
4. Set `channel_lut = None` in `shed_optional_buffers`.
5. Replace the per-channel transform inside the `for channel in
   0..=max_channel` loop with three `lut.lookup` calls.

After this phase the inner loop no longer does `Q32 multiply`,
`to_u16_saturating`, the optional gamma branch, or the surrounding `Q32
→ u8 → Q32 → u16` round-trips per channel.

**Out of scope (do NOT touch — these are deferred to a follow-up plan
explicitly):**

- Hoisting `ctx.get_output(...)` out of the per-channel loop. The
  `lut.lookup` calls happen inside the loop, but `ctx.get_output` and
  `self.color_order.write_rgb_u16` continue to be called per channel,
  same as today.
- Caching `lamp_colors` or any accumulator `Vec`s on `self`.
- Devirtualizing the `TextureSampler` dispatch.
- Sampler-level changes (`Rgba16Sampler::sample_pixel` etc.).
- Any change to `accumulation.rs`, `entry.rs`, `points.rs`,
  `sampling/`, or `overlap/`.
- Eagerly building the LUT in `init` (deliberately lazy — see Q5 in
  `00-notes.md`).

If you find yourself wanting to do any of these — **stop and report**.

## Code organization reminders

- `runtime.rs` is already ~947 lines. Don't grow the file unnecessarily.
  All the LUT logic lives in `channel_lut.rs`; this phase only wires it
  in.
- Group the new field with adjacent transient state (the existing
  `mapping`, `precomputed_mapping`, etc. fields).
- The `update_config` change is local: read old values, compare, write,
  invalidate.
- Keep the inner render loop tight — no clever helper extraction in
  this phase. Phase 05 may tidy if it's warranted.

## Sub-agent reminders

- Do not commit until the main agent has reviewed the diff.
- Do not expand scope (see "Out of scope" above — that list is
  load-bearing).
- Do not suppress warnings or `#[allow(...)]`.
- Do not weaken the exhaustive sweep tests in
  `nodes::fixture::channel_lut::tests`.
- If you cannot make the new integration test pass, **stop and report**
  — that probably means the inner loop's gamma branch order or the
  `lamp_colors >> 8` byte derivation diverges from what `ChannelLut`
  produces.
- Report back: files changed, validation output, deviations.

## Implementation details

### File: `lp-core/lp-engine/src/nodes/fixture/runtime.rs`

#### 1. Import

Near the existing `use super::*;` / `use super::gamma::*;` block, add:

```rust
use super::channel_lut::ChannelLut;
```

The phase-03 `apply_gamma` import is no longer needed inside `render()`,
because gamma is now collapsed into the LUT. **However**, do not remove
the `apply_gamma` import yet unless it becomes genuinely unused — check
the full file with the compiler. If clippy/rustc say it's unused, remove
the import. If something else still uses it, leave it.

#### 2. Field

Add to `FixtureRuntime`:

```rust
/// Lazily built per-channel transform LUT. Invalidated when
/// brightness or gamma_correction changes; shed by
/// shed_optional_buffers.
channel_lut: Option<ChannelLut>,
```

Update the `impl Default for FixtureRuntime` (or wherever the struct is
constructed) to initialize `channel_lut: None`. Look for the `impl
Default` and any `FixtureRuntime { ... }` literal in the file.

#### 3. `render()` — replace the per-channel post-loop body

The current loop (lines ~307–329) is:

```rust
let brightness = self.brightness.to_q32() / 255.to_q32();
// ...
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

Replace with:

```rust
// Build / fetch the per-fixture channel transform LUT. The LUT
// collapses brightness × to_u16_saturating × (optional gamma) into a
// single load. Invalidated by update_config when brightness or
// gamma_correction changes; shed by shed_optional_buffers.
let lut = self.channel_lut.get_or_insert_with(|| {
    ChannelLut::build(self.brightness, self.gamma_correction)
});

for channel in 0..=max_channel as usize {
    let r = lut.lookup(ch_values_r[channel]);
    let g = lut.lookup(ch_values_g[channel]);
    let b = lut.lookup(ch_values_b[channel]);

    // lamp_colors stores the pre-gamma 8-bit byte for state extraction
    // (matches current behaviour: high byte of the saturated u16 BEFORE
    // gamma).
    //
    // CAREFUL: the LUT's u16 output is POST-gamma. To recover the
    // pre-gamma byte we'd need a separate LUT — out of scope for this
    // phase. For now, derive the lamp_colors byte from the LUT's u16
    // output (post-gamma). This is a behaviour change and is documented
    // in the commit message; see the "Behaviour change" note below.
    lamp_colors[channel * 3]     = (r >> 8) as u8;
    lamp_colors[channel * 3 + 1] = (g >> 8) as u8;
    lamp_colors[channel * 3 + 2] = (b >> 8) as u8;

    let start_ch = channel_offset + (channel as u32) * 3;
    let buffer = ctx.get_output(output_handle, universe, start_ch, 3)?;
    self.color_order.write_rgb_u16(buffer, 0, r, g, b);
}
```

Also remove the now-unused `let brightness = self.brightness.to_q32()
/ 255.to_q32();` line above the loop.

##### **STOP — read this before writing the code above**

The original code wrote `lamp_colors` from the **pre-gamma** byte and
the wire output from the **post-gamma** byte. The naive replacement
above would write `lamp_colors` from the **post-gamma** byte too,
because the LUT bakes gamma into the wire output. That's a behaviour
change.

The user's intent for this plan is "collapse into one LUT lookup", and
in practice `lamp_colors` is internal state used for monitoring /
extraction. Two options:

- **Option A (recommended for this phase):** accept the behaviour
  change. Document it in the commit message and in a `// CAREFUL:`
  comment in the code (as above). The behaviour change is: when gamma
  is enabled, `lamp_colors` reflects the gamma-corrected byte instead
  of the pre-gamma byte.
- **Option B:** keep the old semantics by computing the pre-gamma byte
  separately. That requires either two LUTs (pre- and post-gamma) or a
  small inline saturate of the brightness-only product. Defeats some of
  the gain.

**Decision:** go with Option A. The user signed off on collapsing the
transform; `lamp_colors` is a debug/state-extraction output, not a
correctness-critical signal.

If the integration test (below) starts failing because of this, **stop
and report** rather than reverting silently — we want the test to drive
the decision, not the other way around.

#### 4. `update_config()` — conditional LUT invalidation

Current code (lines ~380–384):

```rust
self.config = Some(fixture_config.clone());
self.color_order = fixture_config.color_order;
self.transform = fixture_config.transform;
self.brightness = fixture_config.brightness.unwrap_or(64);
self.gamma_correction = fixture_config.gamma_correction.unwrap_or(true);
```

Replace with:

```rust
self.config = Some(fixture_config.clone());
self.color_order = fixture_config.color_order;
self.transform = fixture_config.transform;

let new_brightness = fixture_config.brightness.unwrap_or(64);
let new_gamma = fixture_config.gamma_correction.unwrap_or(true);
if new_brightness != self.brightness || new_gamma != self.gamma_correction {
    self.channel_lut = None;
}
self.brightness = new_brightness;
self.gamma_correction = new_gamma;
```

#### 5. `shed_optional_buffers()`

Current body (lines ~341–344):

```rust
self.precomputed_mapping = None;
self.mapping.clear();
self.mapping.shrink_to_fit();
Ok(())
```

Add one line above the `Ok(())`:

```rust
self.channel_lut = None;
```

#### 6. `init()`

No eager build. The existing assignment of `self.brightness` and
`self.gamma_correction` is fine; the LUT will lazily build on first
`render()`.

### Integration test (in `mod tests` of `runtime.rs`)

Add a test that drives `FixtureRuntime::render` end-to-end and checks
that the wire output matches what the OLD per-channel computation would
have produced for the same inputs. Use `channel_transform_reference`
indirectly via building a `ChannelLut` and looking up.

Pattern (adapt to whatever existing `runtime.rs` test helpers exist —
look at the existing `mod tests` to see what `RenderContext` mock and
fixture-builder utilities are already present; **do not** invent new
test infrastructure):

```rust
#[test]
fn render_lut_matches_per_channel_transform() {
    // 1. Build a fixture with brightness=64, gamma=true.
    // 2. Drive a render() with synthetic accumulator values.
    // 3. Assert the wire output (read back via the mock RenderContext)
    //    equals ChannelLut::build(64, true).lookup(acc_value) for each
    //    channel.
}
```

If the existing tests in `runtime.rs` don't have a usable
`RenderContext` mock or fixture-builder, **stop and report** — adding
that infrastructure is bigger than this phase, and we'd want to discuss
either deferring the test or scoping its setup. Do not silently grow
the phase.

### Acceptable test alternative (if mock setup is too heavy)

If wiring up a full integration test exceeds the phase scope, the
phase's existing safety net is:

- Phase 03's exhaustive sweep test (LUT == reference for all 65_536
  combinations).
- The pre-existing `runtime.rs` tests (which presumably already cover
  the render path).
- A new unit-level test in `runtime.rs` that just constructs a
  `ChannelLut`, calls `.lookup()` for a few values, and compares
  against a re-implementation of the old `Q32 → ×brightness → u16 →
  gamma → u16` chain — no `FixtureRuntime` involved.

Pick the smaller of the two. If the integration test is feasible,
prefer it. If not, do the unit-level comparison test instead.

## Validate

```bash
cargo test -p lp-engine --lib nodes::fixture
cargo test -p lp-engine
cargo clippy -p lp-engine -- -D warnings
```

If anything in the wider crate test suite fails because `lamp_colors`
behaviour changed (Option A above), **stop and report** so we can
decide whether to update the failing test or pivot to Option B.

## Commit

After main-agent review:

```bash
git add lp-core/lp-engine/src/nodes/fixture/runtime.rs
git commit -m "$(cat <<'EOF'
perf(lp-engine): wire ChannelLut into FixtureRuntime render

- Replace per-channel Q32×brightness → to_u16_saturating →
  (optional gamma) → u16 chain with three ChannelLut::lookup calls.
- channel_lut: Option<ChannelLut>, lazily built on first render via
  get_or_insert_with.
- update_config invalidates the LUT only when brightness or
  gamma_correction actually change (cheap equality check).
- shed_optional_buffers drops the LUT.

Behaviour change: when gamma_correction is enabled, lamp_colors now
records the gamma-corrected high byte instead of the pre-gamma high
byte. The wire output is unchanged. lamp_colors is internal state for
extraction / monitoring; this aligns it with the actual bytes shipped
to the strip.

Plan: docs/plans/2026-04-19-fixture-render-perf/04-wire-channel-lut.md
EOF
)"
```

## Capture profile

```bash
cargo run -p lp-cli --release -- profile examples/perf/fastmath --note p4-channel-lut
ls -dt profiles/*--p4-channel-lut | head -n 1
```

Report back the top 10 from `report.txt`. Specifically check:

- `FixtureRuntime::render` self-cycles dropped.
- `apply_gamma` / `Q32::to_u16_saturating` / `Q32` mul helpers shrunk.
- The new `ChannelLut::lookup` shows up but is much cheaper than what
  it replaced (it should be a single inlined load).
