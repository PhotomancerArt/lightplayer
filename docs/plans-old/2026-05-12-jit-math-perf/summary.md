# Summary

Implemented a data-driven JIT math perf pass for steady-state rendering.

Shipped:

- ESP32-C6 firmware microbenchmark harness for Q32 mul/div/trig/LUT candidates.
- Fast-by-default Q32 compiler/model options.
- Fast parabolic `__lps_sin_q32` and matching `sincos` path.
- Regenerated psrdnoise LUT and LPFN snapshots affected by sine.
- Updated scalar divide filetests for reciprocal default behavior.
- Refreshed `examples/rocaille` metadata so it profiles under the current domain model.
- Captured results in `docs/reports/2026-05-12-jit-math-perf.md`.

Not shipped:

- Inline reciprocal divide. A quick native-backend attempt failed RV32 filetests
  and was reverted. It remains the best next measured target.
