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

The 1200/1500 failures are the ~25.6 B/LED contiguous mapping-slot
expansion (`generate_mapping_points` ← `ensure_direct_channels`) meeting a
load-churned heap. Marginal steady cost measured 600→900 is ~100 B/LED
all-in — the parked M6 "compact resolved carrier" (−24 B/LED) is what
makes 1500 fit outright.

## 2026-08-04 (late): M6 compact mappings — 1500 RUNS

On branch `claude/m6-compact-mappings-f98a49` (clone kill + streaming
visitor + compact resolved carrier + drop recast), same probe cadence:

| LEDs | result | fps | tick | steady used | largest_free |
|---|---|---|---|---|---|
| 1200 | ✅ compile 64 ms, renders | 14 | 66 ms | 119,596 B | 56,045 |
| 1500 | ✅ **compile 69 ms, renders** | **12** | **76-77 ms** | **137,332 B** | 38,883 |

`retry_saves=0` on both — nothing needed rescuing. The frame model
(24 ms fixed + 5 µs/LED render + 30 µs/LED sequential flush) predicted
66 / 76.5 ms; measured 66 / 76-77. Post-M6 marginal ≈ 59 B/LED
(1200→1500). fps at 1500 is flush-bound: M4 concurrent flush is the
next lever (~23 fps projected).

Emulator A/B on this project (archived in `profiles/`): load retained
89,875 → 37,231 B (the 61,440 B slot-modelled mapping → ~9 KB compact),
load transient 140,545 → 73,986 B, per-frame churn −73% (the 36 KB/frame
mapping clone is gone).

## Same-day follow-up (earlier): retrying allocator + 24 KiB JIT region

Two levers landed on this branch and were re-probed (total heap
178,176 → 186,368 B; `[MEM]` now carries `retry_saves=`):

| LEDs | result | notes |
|---|---|---|
| 900 | ✅ 17 fps unchanged | steady free 60,416 B (+7.9 KB from the region shrink) |
| 1200 | ❌ first tick | now **genuine fragmentation**: 25 holes, largest 24,888 < 30,720 ask, `retry_ok=false` — the earlier `retry_ok=true` window-free anomaly did not recur; the contiguous mapping vec is the confirmed wall and M6 its fix |
| 900 (heavy shader) | ❌ compile | exhausts even the enlarged heap: used=183,024, 3.3 KB free in crumbs, `retry_ok=false` — "LED count × shader size" stands |

## Uploading

```bash
lp-cli upload projects/test/zook-dome-1500 serial:auto
```

⚠️ At 1500 this OOM-crashes a classic twice and quarantines itself (by
design). `espflash reset` is the power-on-class ledger wipe; erase the lpfs
region (`0x310000 0xF0000`) to remove the project entirely.
