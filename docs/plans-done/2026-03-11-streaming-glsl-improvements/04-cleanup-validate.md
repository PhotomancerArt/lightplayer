# Phase 4: Cleanup & Validate

## Validate

After applying phases 1-3, re-run on ESP32 emulator with heap tracing and
compare to the before-streaming baseline (99,422 bytes free at peak).

The streaming path should now show a net improvement over the baseline.

Expected at peak:

- JITModule::declare_function should be ~34 KB (two modules, unavoidable)
- GlModule::declare_function should be gone or near-zero
- glsl_jit_streaming overhead should be reduced (no map cloning)
- T::clone_one should be gone or reduced

```bash
cd lp-shader/lp-glsl-compiler && cargo test --features std
cd lp-shader/lp-glsl-compiler && cargo check --no-default-features --features core
cargo +nightly fmt
```

## Cleanup

Grep for TODOs and temporary code in the streaming implementation:

```bash
git diff --name-only | xargs grep -n "TODO\|FIXME\|HACK\|println!\|dbg!" 2>/dev/null
```

Update `docs/plans/2026-03-10-streaming-compilation/00-notes.md` with findings.

Move plan to `docs/plans-done/` when complete.
