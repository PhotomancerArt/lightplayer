# Plan E: Validation & Cleanup — Questions

## Context from review of Plans A–D (last 4 commits)

**Dead code found:**
- `backend/transform/`: `apply_transform`, `transform_single_function`,
  `Q32Transform`, `TransformContext`, `Transform` trait, `IdentityTransform`
  — none called from production code, only from transform tests
- `lp_lib_fns.rs`: dead file, not included in `mod.rs` (superseded by `lpfx_fns.rs`)

**Still used from `backend/transform/`:**
- `Q32Options` (from `q32/options.rs`) — used by `numeric.rs`, `executable.rs`, `lib.rs`
- `float_to_fixed16x16` (from `q32/types.rs`) — used by `numeric.rs`

**Stale comments (referencing removed transform):**
- `lpfx_fns.rs` lines 231–234, 253–254: "converted to q32 by transform"
- `builtins/helpers.rs` line 30: "converted to q32 by transform when applicable"
- `glsl_compiler.rs` lines 69, 137, 224, 292: "FLOAT signatures (no conversion)"
- `numeric.rs` line 31: doc says `todo!()` but code uses `unreachable!()`

**Latent issue:**
- `numeric.rs` `float_cc_to_int_cc`: `Ordered` → `IntCC::Equal` and
  `Unordered` → `IntCC::NotEqual` — incorrect for fixed-point (should be
  constant true/false since Q32 has no NaN). Not currently exercised by
  the frontend.

---

## Q1: Relocate Q32Options and float_to_fixed16x16

When we remove the transform, these two items still need a home.

Options:
- **(a)** Move `q32/options.rs` and `float_to_fixed16x16` from `q32/types.rs`
  into `frontend/codegen/numeric.rs` (they're only used there + lib re-export)
- **(b)** Create a new `backend/q32/` module (not under `transform/`) to hold
  the shared Q32 types
- **(c)** Move them into a top-level `q32` module (peer to `backend`/`frontend`)

**Decision:** (b) New `backend/q32/` module. Keeps backend/frontend boundary clean.

---

## Q2: Transform test removal

The transform has ~1000+ lines of tests across many files. With direct
emission validated by filetests, these tests are for code that's being deleted.

Options:
- **(a)** Delete all transform tests along with the transform code
- **(b)** Keep a few key transform tests as regression reference, gated behind
  a feature flag

**Decision:** (a) Delete all transform tests with the transform code. Git history preserves them.

---

## Q3: JIT build path consolidation

The roadmap mentions that `build_jit_executable`,
`build_jit_executable_memory_optimized`, and `build_jit_executable_streaming`
could potentially be consolidated now that there's no transform. Should we
attempt this in Plan E?

Options:
- **(a)** Yes, consolidate now — simpler codebase
- **(b)** No, defer — they serve different purposes (batch vs streaming vs
  memory-optimized) and the consolidation is a separate concern

**Decision:** (b) Defer. The three paths serve different purposes; consolidation is a separate concern.

---

## Q4: Memory / performance measurement

The roadmap lists "run ESP32 heap traces" and "benchmark compilation time"
as Plan E work. Should we include this in the plan, or is it a separate
follow-up activity?

Options:
- **(a)** Include measurement in the plan (run traces, compare before/after)
- **(b)** Separate — the plan focuses on cleanup, measurement is done ad-hoc

**Decision:** (b) Separate. Cleanup plan focuses on code changes; measurement done ad-hoc after.

---

## Q5: float_cc_to_int_cc Ordered/Unordered

The mapping of `FloatCC::Ordered` → `IntCC::Equal` is incorrect for Q32
(should be "always true" since Q32 has no NaN). Not used by the frontend
today, but it's a latent bug.

Options:
- **(a)** Fix now — emit `iconst 1` / `iconst 0` for Ordered/Unordered
- **(b)** Leave as-is with a comment — not exercised, fix when needed

**Decision:** (a) Fix now. Small change, avoids a latent bug trap.
