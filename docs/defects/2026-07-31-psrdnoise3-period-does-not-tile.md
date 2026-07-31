# `lpfn_psrdnoise(vec3)` does not tile when given a period

**Found:** 2026-07-31, during the f32 builtin transliteration (roadmap M5).
**Status:** open. Reproduced, pinned by a test, deliberately not fixed here.
**Affects:** the canonical GLSL, the Q32 device implementation, and the new
native-f32 implementation — all three agree, and all three are wrong.

## Symptom

`lpfn_psrdnoise(vec3 x, vec3 period, …)` with a non-zero `period` does not
repeat at that period. Evaluating at `x` and at `x + period` gives unrelated
values, and the shifted evaluation is frequently exactly `0`.

The 2D sibling `lpfn_psrdnoise(vec2, …)` **does** tile correctly.

## Cause

On the periodic path the corner *offsets* are recomputed from the **wrapped**
corner positions while `x` itself is left unwrapped:

`lps-builtins/glsl/lpfn/generative/psrdnoise/psrdnoise3.glsl`:

```glsl
        // Offsets from the (wrapped) corners.
        x0 = x - w0;
```

and identically in `psrdnoise3_q32.rs`:

```rust
    // Recompute x vectors from wrapped v
    let x0_w = x - v0_wrapped;
```

Wrapping is supposed to affect only the *hash indices*, so that distant cells
reuse the same gradients. Using the wrapped positions for the offsets means a
shift of `x` by one period leaves the corners where they were and moves the
offsets by a whole period — outside the radial support `max(0.5 - |x_k|², 0)`,
so every corner contributes zero.

The 2D canonical gets this right: it wraps only into `iu`/`iv` and keeps
`x0 = x - v0` on the unwrapped corners.

## Why it was not fixed in M5

- The canonical GLSL is normative
  (`docs/adr/2026-07-08-glsl-canonical-builtins.md`), and the Q32
  implementation agrees with it. Fixing only the f32 port would make the two
  float modes disagree while leaving the actual bug in place.
- Q32 builtin changes are explicitly out of scope for the f32 builtin
  milestone.
- The fix is a three-place change (canonical GLSL, Q32, f32) plus whatever
  snapshot/filetest expectations currently encode the wrong output — a
  self-contained piece of work that deserves its own change.

## Pinned by

`psrdnoise3_f32.rs::tests::the_periodic_path_does_not_actually_tile_yet`
asserts the current (wrong) behavior and says in its doc comment that the fix
should delete it and assert tiling instead.

## Suggested fix

Keep the wrapped positions only for the lattice indices; compute `x0..x3` from
the unwrapped `v0..v3`, matching the 2D implementation and the upstream
psrdnoise. Then re-derive any affected snapshots.
