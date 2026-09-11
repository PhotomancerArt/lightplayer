# five-wire

Five wires over four RMT slots, on the DOM-Z-102's own pins — the project the
**second wave** needs in order to exist at all.

`lp-fw/fw-esp32v3`'s block plan caps the pool at `POOLED_SLOT_CAP = 4`
two-block slots (`v3_rmt.rs`), so a fifth wire cannot have a slot of its own:
it time-shares one by per-transmission pad muxing, a `func_out_sel_cfg` write
between waves (`wire_pusher.rs`). That second wave only runs on the product
path, driven by mailbox posts from the PRO core, so the only way to exercise
it is a project that really declares five outputs. This is that project.

| Port | Endpoint | Lamps | Channels |
|---|---|---|---|
| 0 | `ws281x:local:IO18` | 16 | 0..16 |
| 1 | `ws281x:local:IO16` | 16 | 16..32 |
| 2 | `ws281x:local:IO14` | 16 | 32..48 |
| 3 | `ws281x:local:IO2` | 16 | 48..64 |
| 4 | `ws281x:local:IO13` | the rest (16) | 64..80 |

The four fused DATA terminals plus `IO13`, the claimable spare — the board's
own five-wire shape (`lp-core/lpc-hardware/boards/domraem/dom-z-102.json`, and
its `measured` note: *5 wires × 300 LEDs at 29.99 fps, 240 s soak on the
dual-core pusher build*). Only the highest-keyed port may omit its `count`; it
means "the rest of the buffer".

## The three constraints, and what breaks if you relax one

**No clock, and no `time` input.** `shader.glsl` is
`projects/test/shader-oracle/shader.glsl`, byte for byte, and for the same
reason: every frame is identical, so *"every wire's frames are the same
frame"* is a statement about the machine rather than about when it was
sampled. Add a clock and a per-frame checksum comparison stops meaning
anything.

**Five distinct wires.** The five paths sample five different rows of the
render, so the five wires carry five different byte strings. A routing
mix-up — two pads driven from one channel, a slot handed to the wrong wire —
then shows up as two wires with the same checksum, which is exactly the
failure the second wave can cause. Give every row the same colour and that
evidence disappears.

**A neutral output pipeline.** `white_point [1,1,1]`, brightness 1, LUT,
dithering and interpolation all off, gamma off — the same neutral setting the
oracle project documents. Under exactly that configuration `DisplayPipeline`
is stateless, so the bytes on the wire are the engine's samples and a frame
is a function of the project alone.

## Where it is used

- `lp-emu/esp/lp-emu-esp32v3/tests/five_wires.rs` — the emulator's five-wire
  gate: five pads routed, a re-mux visible between waves, and each wire's
  decoded frames checksum-equal to that endpoint's own
  `[OUT] frame=… crc=` summary line.
- `lp-emu/esp/lp-emu-esp32v3/walks/five-wire.script` — the `lp-cli upload` of
  this project, captured live and replayed in guest time.

It is not a demo. Sixteen lamps a wire is deliberately small: the whole point
is the slot arithmetic, and a wire long enough to be pretty costs emulated
seconds without changing what is being measured.
