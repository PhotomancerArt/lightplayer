# Penta strands v3 — one output node, five wires

The multi-channel bring-up project: **one** fixture authoring five paths and
**one** output node whose `channels` map splits that fixture's single control
buffer across five wires. `projects/test/quad-strips-v3` is the shape this
replaces — four nodes with one channel each — and it stays valid; this project
is the same picture drawn the new way.

## Wiring (DOM-Z-102 silkscreen labels)

| Channel | Endpoint | Lamps | Buffer samples | Band color |
|---|---|---|---|---|
| 0 | `ws281x:local:IO18` | 4 | 0..12 | red |
| 1 | `ws281x:local:IO16` | 4 | 12..24 | green |
| 2 | `ws281x:local:IO14` | 4 | 24..36 | blue |
| 3 | `ws281x:local:IO2` | 4 | 36..48 | amber |
| 4 | `ws281x:local:IO13` | remainder | 48..60 | violet |

Channel 4 deliberately omits its `count`: only the highest-keyed channel may,
and it means "the rest of the buffer". Slices are derived cumulatively in key
order, so the counts above are the whole authoring — nothing names an offset.

## ⚠️ Five channels is a host figure

The classic ESP32 declares **four** concurrent RMT channels in its board
manifest, and a fifth `/rmt/ws281xK` must never be declared (the 40 µs refill
foot-gun). On the desk board the fifth wire parks until a channel frees up;
the hardware walk drives four. Five exists here because host tests and the
studio face need a project where the channel count is not the same as the
node count.

## Uploading

```bash
lp-cli upload projects/test/penta-strands-v3 serial:auto
```
