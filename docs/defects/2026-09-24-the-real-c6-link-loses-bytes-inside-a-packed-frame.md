---
status: fixed
found: 2026-09-24      # hardware walk: the JSON Pack desk sitting (G1 of lp2025/2026-09-23-1701-lp-json-pack)
area: fw-esp32c6 USB-Serial-JTAG write path (esp-hal usb_serial_jtag) × lp-emu-esp-common ip/usb_sj.rs (the link model)
class: fidelity
related:
  - docs/defects/2026-09-13-the-s3-link-drops-the-io-tasks-next-chunk-on-a-stale-serial-in-empty.md
  - docs/defects/2026-08-02-serial-line-interleaving.md
  - docs/adr/2026-09-24-json-pack-wire-encoding.md
  - https://github.com/PhotomancerArt/lightplayer/pull/805
---
# The real C6's USB link loses a few bytes inside a packed frame; the emulated one never does

**Symptom.** On a desk XIAO ESP32-C6 (MAC `a0:f2:62:87:b4:8c`, the PLAYFUL
choker, branch `feat/lp-json-pack` at `d6412ade8`), Studio's lens was captured
for a few minutes with `?wire-capture=1&device-log=info` at three lens pauses.
`lp-cli wire unpack --sizes` over the three captures found 349 packed frames:

| capture | frames | decode errors |
|---|---:|---:|
| 150 ms | 118 | 4 |
| 75 ms | 110 | 0 |
| 33 ms | 121 | 0 |

Three of the four are at the start of the connection. The board logged its own
writes timing out while the host was not yet reading
(`[io_task] server frame USB write timed out at chunk 1/1 (0 of 172 B) after
250 ms`, `dropping message id=0`), so those frames were started and abandoned.
The board says so, and the scanner resyncs at the next `0x00`.

The fourth is the defect. Frame #66 of the 150 ms capture, at byte 92,935, is
a 1,596-byte packed body with no console text inside it. Its COBS walk
overruns the body by 5 bytes, so it arrived about **5 bytes short**, and the
next frame begins cleanly right after it. The board logged nothing. The only
nearby sign is the `[perf]` line just after it, reporting 17 fps against the
steady 36–37. Studio logged a torn frame and carried on. That one lens reply
was lost.

The emulator walks of the same image (`walk-esp32c6-emu`, `walk-no-board`,
`walk-no-board --tab`) decoded every frame, with 0 errors.

**Mechanism (not proven).** The likeliest candidate is the one the S3 defect
describes: esp-hal's USB-Serial-JTAG write future resolves early on a stale
`serial_in_empty`, and the next chunk is written into an IN FIFO that is not
free. That defect is open because nobody knew whether silicon drops such a
write. This capture is the first silicon observation of a silent loss on a
USB-Serial-JTAG link, but it lost ~5 bytes, not a 64-byte packet, so it is
evidence, not proof. The host side (the Web Serial read pump) cannot be ruled
out from one capture.

**Since filed: PR #805** (the S3 link fix) settled from primary documents
that silicon refuses a write while an IN packet is pending (ESP32-C3 TRM
v1.3 §30.3.2 p. 767, the same USB-Serial-JTAG block, and the C6/S3 PAC text
for `SERIAL_IN_EP_DATA_FREE`). It also found that esp-hal 1.1.1's
`write_async` breaks that rule twice: it writes a chunk without checking the
endpoint is free, and it wakes on a stale `serial_in_empty`. #805 fixes the
S3 only. The C6 runs the same driver and was deliberately left out. So the
stale-wake race is a **candidate cause** here, not a finding. The ~5-byte
loss does not match the emulator model, which drops the whole write, but no
document says what silicon does with bytes written into a pending buffer,
so a partial loss is plausible.

**The test that would tell us:** repeat the capture on a C6 image carrying
#805's IN-endpoint gate (ported, below: now
`lp-fw/fw-esp32-common/src/serial/in_endpoint.rs`). If the loss goes away, it was this race.

**Packed vs JSON on the same board (2026-09-24, same afternoon).** The
same XIAO and the same image, Studio at a 150 ms lens pause, captured with
`?wire-capture=1`:

| run | messages | wire bytes | lost/damaged |
|---|---:|---:|---:|
| packed (three G1 captures + one 3-min run) | ~1,400 frames | ~0.95 MB | 4 in steady state (short frames, one `BadTag(255)`) |
| JSON (board reset so it never packed; 9 min) | 1,270 `M!` lines | 2.84 MB | **0** |

At the packed rate (~0.3 % of messages), JSON should have lost about 4, and
0 happens about 1 time in 40. So the loss is **specific to the packed path**,
not a per-byte property of the link. Studio's side is ruled out: both runs
use the same raw read pump, and the capture is taken before any splitting.
That leaves the board's packed write path. Packed frames skip the measuring
pass and go out sooner, in smaller writes, which is the pattern that would
trigger the stale-`serial_in_empty` race above more often. Not yet proven.

**Fix applied, pending hardware confirmation (2026-09-24, PR #795).**
#795 merged #805 and ported its gate to the C6. There is one gate, in
`lp-fw/fw-esp32-common/src/serial/in_endpoint.rs`, generic over the TX
half with the two register touches injected per chip
(`UsbSerialJtagInEndpoint` in `board/<chip>/usb_connection.rs`). The C6 and
S3 io_tasks wrap their whole TX half in it, so every byte the io_task writes
passes it: JSON `M!` lines, packed frames out of `FRAME_BUF`
(`lpc_wire::ser_packed_frame_to`), log lines and probes. The gate also hands
esp-hal at most one 64-byte packet per `write`, so it runs before **every
packet** of a multi-chunk frame, not just before each 256-byte
`ChunkedWriter` chunk. esp-hal's inner loop checks nothing between packets.
Cost: C6 image +288 B (2,478,832 → 2,479,120 B, headroom 666,608 B);
`.bss` +24 B, so the C6 stack total is 71,072 B (was 71,096), re-baselined
in `scripts/heap-budget-record.json`. Heap figures unchanged.

Emulator evidence (`lp-emu-esp32c6` from this worktree, `t1`, emulated
only): `emu_usb_json_pack` decodes every packed frame, and
`walk-esp32c6-emu` passes, on the gated image. **A loss could not be
reproduced without the gate.** After boot, the shipped C6 has a single
writer on the IN endpoint: esp-println carries only the `[INIT]` chain and
the panic path, and every log line rides the io_task. esp-hal's own write
future then always wakes on its own packet's drain, so the model never
refuses a write and the gate is a no-op in steady state. The pre-gate branch
passed the same tests. Three C6 tests that pinned the pre-gate writes into a
held packet (`usb_attached::g2_3`, `usb_control::g3_1b`, `host_absent`)
were re-pinned to the gate's waits, and none replays a transcript.

That is a reason for doubt. On the emulator's model, the stale-
`serial_in_empty` race needs a second writer, and the C6 has none in steady
state. So if the desk re-capture still shows short packed frames, the cause
is something the model doesn't have. Candidates: silicon raising
`serial_in_empty` at a moment other than the drain, the exact-64-byte packet
followed by a redundant `wr_done`, or a host-side cause after all.

**Confirmed on hardware (2026-09-25, `035fe5fed`).** The same XIAO, flashed
with the gated image. Studio's lens ran packed at the new 75 ms pause for 8
minutes with `?wire-capture=1&device-log=info`:

| image | packed frames | wire bytes | lost/damaged |
|---|---:|---:|---:|
| before the gate (all packed runs above) | ~1,400 | ~0.95 MB | 4 |
| with the gate | **1,327** | 948 KB | **0** |

At the pre-gate rate, 0 in 1,327 would happen by chance about 1 time in 50.
The gate is the fix. The emulator never showed the loss, because after boot
the C6 has a single writer and its link model never refuses a write in steady
state. So the fidelity half stays true: the model has no path to this loss on
the C6. A model change is follow-up work, not part of this fix. Capture:
`g1c-gated-packed-75.bin` in the plan directory.

**What is not known yet.**
- Whether the gate makes the packed loss go away on silicon. The test is the
  G1 capture repeated on a gated image: three lens pauses, ~1,400 frames,
  `lp-cli wire unpack --sizes`.
- Whether anything else on the packed write path (the in-place frame build
  in `FRAME_BUF`, the chunking of `\n` + frame) also contributes.

**Why the emulator misses it.** The link model delivers every byte a write
commits, and drops a byte only in the one committed-FIFO case the S3 defect
names. It has no path that produces a short frame at this rate. Per the
emulator-first rule, the fix starts in the model once the mechanism is known.

**Evidence.** `~/.photomancer/planning/lp2025/2026-09-23-1701-lp-json-pack/g1-{150,75,33}.bin`
(raw captures), with `.txt`/`.sizes` from `lp-cli wire unpack --sizes`.
