# Future Work

## Math Debug Probe

- **Idea:** Add a debug probe that injects overflow, divide-by-zero, saturation, and approximation-drift detection into shader math.
- **Why not now:** This spike is about default render speed and on-device measurements, not debug instrumentation.
- **Useful context:** Keep reference helpers in `lps-builtins`; do not make reference math the normal render path again.

## LICM And Uniform Precomputation

- **Idea:** Hoist loop-invariant reciprocals and trig calls out of per-pixel loops once the inliner/middle-end can see them.
- **Why not now:** This is a broader LPIR middle-end project and should not distract from primitive math measurements.
- **Useful context:** See `docs/future/2026-04-20-middle-end-optimization.md`.

## Shader Idiom Rewrites

- **Idea:** Recognize common shader idioms such as paired `sin`/`cos`, palette ramps, and repeated literal divisors.
- **Why not now:** Requires more frontend/middle-end pattern work after primitive costs are known.
- **Useful context:** `docs/future/2026-05-03-rocaille-fastmath-profile.md` calls out paired/hoisted trig and constant division.

## Hardware Counter Automation

- **Idea:** Add host-side parsing for `[jit-math-perf]` serial output so hardware runs produce structured CSV/Markdown automatically.
- **Why not now:** Manual paste is sufficient for the first spike; automation only pays off if this becomes a recurring lab.
- **Useful context:** The output format should remain stable in Phase 1 to make this easy later.
