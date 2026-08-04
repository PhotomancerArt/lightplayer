# Zook dome 1500 — the single-controller capacity project

Dome-scale measurement project for the 2026 Zook dome use case: **1500 LEDs
on one classic ESP32 (DOM-Z-102)**, authored at the real topology — one
fixture, one output node.

- **Fixture**: Map2d source, 5 grid strands of 300 lamps (the dome's
  physical shape). Direct sampling, gamma on, brightness 0.15 (desk-safe).
- **Output**: 4 wires of 375 lamps (IO18/IO16/IO14/IO2 — the four
  concurrent RMT channels), so every lamp actually transmits. The physical
  dome is 5×300; a 5th wire parks until a channel frees (multi-channel
  roadmap M6), so measurement splits the buffer 4 ways.
- **Shader**: the penta-strands band-chase (1,276 B GLSL) — dome-realistic
  "pretty simple shader". For a heavy-shader bound, swap in
  `examples/basic/shader.glsl` (4,092 B, psrdnoise).

## Measured on silicon 2026-08-04 (fw f799ee61e = merged main)

Scaled copies of this project (`set_dome_count.sh` in the session notes;
same files, smaller grids/wires):

| LEDs | result | fps | tick | steady used | notes |
|---|---|---|---|---|---|
| 600 | ✅ runs | 21 | 45 ms | 95,512 B | compile 64 ms, 2,116 B JIT |
| 900 | ✅ runs | 17 | 55 ms | 125,664 B | compile 64 ms; 52.5 KB free, unfragmented |
| 1200 | ❌ first tick | — | — | — | `alloc 30720` (mapping vec) fails, `free=50004 largest_free=22264 retry_ok=true` |
| 1500 | ❌ first tick | — | — | — | `alloc 38400` fails, `free=42548 largest_free=12160 retry_ok=true` |
| 900 (heavy shader) | ❌ compile | — | — | — | `alloc 768` fails at `used=177072` — genuine exhaustion |

The 1200/1500 failures are the `retry_ok=true` allocator edge
(`docs/debt/2026-08-02-classic-oom-retry-succeeds.md`) hitting the
~25.6 B/LED contiguous mapping-slot expansion
(`generate_mapping_points` ← `ensure_direct_channels`): the block fits on
retry, but the failed alloc is treated as a crash and two boots quarantine
the project. Marginal steady cost measured 600→900 is ~100 B/LED all-in,
so 1500 extrapolates to ~186 KB against the 178,176 B arena — the parked
M6 "compact resolved carrier" (−24 B/LED) is what makes 1500 fit outright.

## Uploading

```bash
lp-cli upload projects/test/zook-dome-1500 serial:auto
```

⚠️ At 1500 this OOM-crashes a classic twice and quarantines itself (by
design). `espflash reset` is the power-on-class ledger wipe; erase the lpfs
region (`0x310000 0xF0000`) to remove the project entirely.
