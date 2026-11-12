## Debug `lp-cli profile` finalization stall

- **Idea:** Investigate why `lp-cli profile examples/perf/fastmath --mode steady-render --collect events` can reach max cycles after project load/JIT/frame driving but then appear to stall during profile finalization.
- **Why not now:** Demo and device validation showed the migrated `node.toml` source path works; the remaining issue appears isolated to profiler teardown rather than source loading or runtime behavior.
- **Useful context:** M3 validation observed the profiler reach project sync, load, shader JIT, and steady-render frames before stalling after the max-cycle warning. See `docs/roadmaps/2026-05-01-runtime-core/m3-legacy-source-migration/summary.md`.
