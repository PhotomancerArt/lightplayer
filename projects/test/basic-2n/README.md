# Basic 2n (and 4n, 2n-half) — node-count siblings of `projects/test/basic`

Three generated projects that exist for one measurement: **what does one
fixture+output pair cost the engine?** `per_lamp_memory_table.rs` answers the
per-*lamp* question by taking a slope over lamp count; these take a slope over
**node count** with everything else held identical to the parent.

| dir | pairs | lamps/pair | total lamps | purpose |
|---|---:|---:|---:|---|
| [`basic-2n`](.) | 2 | 241 | 482 | slope point |
| [`basic-4n`](../basic-4n) | 4 | 241 | 964 | slope point |
| [`basic-2n-half`](../basic-2n-half) | 2 | 121 | 242 | same node count as `basic-2n`, half the lamps — subtracting it isolates the per-lamp part of the per-pair slope |

The parent, `projects/test/basic`, is the 1-pair point: one clock, one shader, one
fixture (241 lamps: a 1×1 centre grid plus a 240-lamp 8-ring disc on a 10×10
canvas, Direct sampling), one output (interpolation on, LUT on).

## The rules

- Pair *i* is `fixture{i}.json` + `fixture{i}.map2d.json` + `output{i}.json`,
  each byte-identical to the parent's except:
  - `fixture{i}.json`: `mapping.source` → `fixture{i}.map2d.json`,
    `bindings.output.target` → `bus:control.out/ch{i}`;
  - `output{i}.json`: port 0 `endpoint` → the XIAO ladder `D10, D9, D8, D7`
    (pair 1 keeps the parent's `D10`), `bindings.input.source` →
    `bus:control.out/ch{i}`;
  - `fixture{i}.map2d.json`: the parent's map, with every ring count halved in
    `basic-2n-half` (`[30,24,20,16,12,8,6,4]` = 120 + centre).
- Per-channel bus names follow [`../quad-strips`](../quad-strips).
- `clock.json`, `shader.json` and `shader.glsl` are the parent's, copied once
  per project; `module.json` lists every node; `project.json` carries a
  distinct `name`.
- The `D`-labels are the emulator's permissive hardware manifest (the XIAO
  ESP32-C6 board profile); `P0..`-style endpoints load but never *open* there,
  which silently drops the per-port buffers from the measurement. These
  projects are measurement fixtures, not bring-up projects — nothing here is
  meant to be uploaded to hardware.

## Regenerating

**Change `projects/test/basic` → regenerate.** The whole point is that the three
siblings differ from the parent only in node count; a hand edit here breaks
the slope silently.

```bash
python3 projects/test/basic-2n/generate.py
```

Python 3 stdlib only, deterministic, writes all three directories. ⚠️ Node-def
JSON is read with `kind` as a leading header, so the script preserves key
order and never sorts keys — except the `nodes` map in `module.json`, which it
emits sorted by name because that is the canonical writer's order and
`lp-cli/tests/examples_valid.rs` holds every checked-in rig to those bytes.

## Who uses them

`docs/reports/2026-09-06-classic-not-enough-heap.md` — the per-node cost
attribution section (bytes per fixture+output pair per owner, device width
from `lp-cli profile --collect alloc` and host slopes from
`lp-core/lpc-engine/tests/per_node_memory_table.rs`). That test regenerates its
own temp copies from `projects/test/basic` by these same rules, so the two
instruments cannot drift.
