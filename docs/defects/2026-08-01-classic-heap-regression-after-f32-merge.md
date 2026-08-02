# Classic ESP32: per-project heap grew 8,136 B on main, cutting the LED ceiling

- **Date:** 2026-08-01
- **Status:** OPEN — measured and bracketed, not yet attributed to a single PR
- **Board:** DOM-Z-102 (classic ESP32 rev v3.1), `fw-esp32v3`
- **Found by:** M6/M4-P3 re-measurement after merging `origin/main` into
  `claude/infallible-bose-84a52e`

## Symptom

`projects/test/quad60-v3` (4 channels × 60 = **240 LEDs**) ran on this board
with 7,384 B of heap to spare before the merge. After merging main it **OOMs**:

```
cause=oom  "alloc 360 bytes failed"  →  safe mode
```

The board recovers correctly (the `lp-recovery` ledger attributes it, gates the
path, and stays reachable at 889 fps) — but the configuration that passed
M4-P3 no longer fits.

## Measurement

Identical project (`quad-strips-v3`, 4 × 30 = 120 LEDs), clean boot with
auto-load, same board:

| | pre-merge | merged main | delta |
|---|---|---|---|
| free heap | 18,128 B | **9,992 B** | **−8,136 B** |
| used | 94,508 B | 102,644 B | +8,136 B |
| fps | 13 | **20** | +54 % |
| `tick` | 69 ms | 47 ms | −22 ms |

**Idle heap is unchanged** — 102,156 B free before, 102,144 B after, with no
project loaded. So this is **not** static/`.bss` growth; it is per-project
allocation. Something the project-load or render path allocates now costs
~8 KB more, and simultaneously runs ~30 % faster.

That pairing — faster and fatter — is the signature of work being cached or
precomputed rather than recomputed per frame.

## Not the gamma fix

PR #252 (16-bit gamma) was isolated on identical firmware by flipping
`gamma_correction` on the same board:

| | fps | `tick` | free | used |
|---|---|---|---|---|
| gamma **on** | 19 | 49 ms | 6,268 B | 106,368 B |
| gamma **off** | 20 | 48 ms | 6,264 B | 106,372 B |

**4 bytes of heap and ~1 fps** — inside noise. `GAMMA16` is a `const` in
`.rodata`; it costs flash (image 1,707,792 → 1,720,448 B, which also includes
main's other changes) and no heap. Gamma is exonerated.

## Candidates

Six PRs merged into main since `08779e059`:

| PR | subject | prior |
|---|---|---|
| #249 | f32 native math roadmap | **likely** — changes shader codegen |
| #251 | f32 probe2 capture | likely |
| #253 | M8 xtn f32 targets | likely |
| #250 | hardware board selection | unlikely (UI/metadata) |
| #236 | boards catalog page M3 | unlikely (UI/metadata) |
| #252 | 16-bit gamma | **ruled out by measurement** |

The three f32 PRs are the natural suspects: hardware-float shader execution
would plausibly both speed up `tick` and change what the JIT/engine holds
resident. Note memory `f32-native-math-roadmap` records "+65,680 B for an
unreachable path" on the S3 — that was *flash*, and this is *heap*, so it is a
different measurement, not the same one resurfacing.

**Not bisected.** Doing so needs a firmware build + flash + upload + read per
point (~8 min), and `quad-strips-v3` / `quad60-v3` do not exist on main, so
each point also needs the project copied in. Left for whoever owns the f32
work rather than guessed at here.

## Why it matters

The classic's binding constraint is heap, not flash or RMT
(`docs/adr/2026-08-01-esp32v3-flash-budget.md`). At ≈89.5 B per LED, 8,136 B is
**~91 LEDs of capacity** — roughly a third of the chip's usable budget, gone
without a compensating feature on this chip. It moved the measured ceiling from
"~240 comfortable / ~300 at the edge" to somewhere between 120 and 240.

The S3 and C6 have the arena to absorb it and will not notice, which is exactly
why it needs recording here: the classic is the family's canary for per-project
heap growth, in the same way it was the canary for the per-channel white-point
LUT.

## Reproduce

```bash
just build-fw-esp32v3
espflash flash --chip esp32 --port <port> --partition-table lp-fw/fw-esp32v3/partitions.csv \
  --flash-size 4mb --baud 921600 --after hard-reset \
  target/xtensa-esp32-none-elf/release-esp32v3/fw-esp32v3
espflash erase-region --port <port> 0x310000 0xF0000
cargo run -p lp-cli -- upload projects/test/quad60-v3 serial:<port>
```

Read the board's heartbeat `memory` field. To read without reflashing, the fd
must be held open across `stty` — a bare `stty` then `cat` reopens the port and
loses the baud:

```bash
exec 3<> /dev/cu.wchusbserial1130
stty -f /dev/cu.wchusbserial1130 921600 raw -echo clocal
timeout 30 cat <&3 > out.log
exec 3<&-
```

## Update 2026-08-01 (late): the regression compounded

Re-measured during the RMT-priority plan's P4 classic baseline, on a branch
carrying everything merged through #266: `quad60-v3` (240 LEDs) — which ran
with 7,384 B free at G-M4 and OOM'd after the f32 merges — **now fails at
project LOAD**: `alloc 360 bytes failed, free=904, used=111736`. Free heap
at the failure point dropped from ~7.4 KB to ~0.9 KB, so merges since the
first measurement (candidates: 16-bit gamma #252, linear brightness #265,
io_task JSON lift #245) consumed roughly another 6.5 KB of per-project heap.
Two independent increments now total ~15 KB (~165 LEDs of capacity) against
the M6 ledger's original numbers. Still unbisected; the classic remains the
family's canary and the per-LED/per-project heap cost now needs an owner
before any WLED-class LED-count claim is republished.
