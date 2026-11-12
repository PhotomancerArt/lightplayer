## Split the remaining coarse frontend step

- **Idea:** Break up the remaining `lps-glsl` frontend hotspot so the largest compile slice is smaller than the current ~`54ms` outlier.
- **Why not now:** This plan focused on heap and runtime retained memory, and the product risk today is memory more than CPU.
- **Useful context:** The final stress trace is `/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/traces/2026-05-14T08-42-36-inc-shader-compile-stress.txt`; the worst slice is still frontend tick 4 at `54641us`.
