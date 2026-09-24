---
status: open
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
#805's IN-endpoint gate (`lp-fw/fw-esp32s3/src/serial/in_endpoint.rs`,
ported). If the loss goes away, it was this race.

**What is not known yet.**
- Whether JSON loses bytes at the same rate. A JSON line with bytes missing
  just fails to parse and is dropped, and nothing has ever counted that. A
  packed-vs-JSON comparison capture at 150 ms is queued for when the board is
  free.
- Whether the loss comes from the board or from the host.

**Why the emulator misses it.** The link model delivers every byte a write
commits, and drops a byte only in the one committed-FIFO case the S3 defect
names. It has no path that produces a short frame at this rate. Per the
emulator-first rule, the fix starts in the model once the mechanism is known.

**Evidence.** `~/.photomancer/planning/lp2025/2026-09-23-1701-lp-json-pack/g1-{150,75,33}.bin`
(raw captures), with `.txt`/`.sizes` from `lp-cli wire unpack --sizes`.
