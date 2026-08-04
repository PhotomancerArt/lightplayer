# quad-wire-oracle — the shader-oracle frame, split across four wires

The hardware-walk DEVICE variant for the multi-endpoint output node: the same
clock-free 64-LED render as `examples/shader-oracle`, but the output node's
`channels` map splits that one control buffer across the DOM-Z-102's four
data pins. Because the shader, fixture and pipeline options are byte-identical
to the oracle project, each wire's `[OUT] dump` must equal the corresponding
16-LED slice of `shader_oracle_frame`'s host transcript — no per-project
oracle needed.

| Channel | Endpoint | Lamps | Oracle hex slice (of 384 chars) |
|---|---|---|---|
| 0 | `ws281x:local:IO18` | 16 | 0..96 |
| 1 | `ws281x:local:IO16` | 16 | 96..192 |
| 2 | `ws281x:local:IO14` | 16 | 192..288 |
| 3 | `ws281x:local:IO2` | remainder (= 16) | 288..384 |

Channel 3 omits its `count` on purpose: only the highest-keyed channel may,
and it exercises remainder semantics on silicon.

The board manifest declares exactly four `/rmt/ws281xK` resources — never a
fifth (the 40 µs refill foot-gun, `docs/adr/2026-08-02-classic-hli-refill.md`).

Everything the oracle project's README says still binds here: no clock, the
neutral output pipeline, and exactly 64 LEDs so the four one-shot dumps
together cover every byte of the frame.

```bash
PROJECT=projects/test/quad-wire-oracle scripts/m4-hardware-walk.sh --chip esp32 <port>
cargo test -q -p lpa-server --test shader_oracle_frame -- --nocapture   # host side
```
