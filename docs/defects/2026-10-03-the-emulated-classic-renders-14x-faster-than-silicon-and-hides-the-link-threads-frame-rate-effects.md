---
status: open
found: 2026-10-03      # how: hardware-walk — PR #943's classic desk sitting (desk-classic.md), agent-run, against same-ELF emulator runs
area: lp-emu/esp/lp-emu-esp32v3 time (`t1` only — one cycle per instruction, no flash-cache cost) × frame-relative claims made on it
class: fidelity
related:
  - docs/defects/2026-10-01-the-emulated-c6-charges-a-cold-code-path-10x-less-than-silicon.md
  - docs/reports/2026-09-11-esp32v3-emulator-walk.md   # §3: the classic's t1 reports fewer µs than silicon (0.11–0.26×)
  - docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md
  - lp2025/2026-10-02-1918-io-thread-other-boards
---
# The emulated classic renders a frame ~14× faster than silicon, so the link thread's frame-rate effects are invisible there

**Symptom** — the same ELFs, the same project (`projects/test/quad-wire-oracle`,
DOM-Z-102's four outputs), measured with `lp-cli link rtt`:

| | main `d68791d96` idle fps | PR #943 `96dfc8cca` idle fps | Δ | request p50 / p90 (frames) main → branch | link RTT p50, request phase (ms) main → branch |
|---|---:|---:|---:|---|---|
| silicon, DOM-Z-102 on its CH340K (3 runs each) | 30.86 / 30.86 / 30.87 | 33.20 / 33.19 / 33.18 | **+7.6 %** | 1.92 / 2.12 → 1.05 / 1.23 | 26.4 → 3.8 |
| `lp-emu:esp32v3:t1` (lp-emu `c042102d9`, scaled rig) | 447.71 | 447.51 | −0.04 % | 2.35 / 2.80 → 1.68 / 2.35 | 4.0 → 3.5 |

The scaled emulator rig (`--warmup-s 2 --idle-s 10 --reads 1 --writes 1
--count 40 --tail-s 1`) is the same for both images; a default-length run
on this project does not finish in 10 minutes of wall time.

A frame is 32.4 ms on silicon and 2.23 ms emulated — **14.5×**. The link's
own round trip is ~4 ms on both, so the ratio of link time to frame time —
which is what every frame-relative claim in PR #943 rests on — differs by an
order of magnitude between the two:

- **Idle frame rate.** The emulator said the link thread costs nothing at
  idle (−0.04 %; P4: "within 0.05 %"). Silicon says it *gains* 7.6 %
  (step-1, the event-driven idle wake on main's executor, alone: +6.4 %,
  32.84 fps, on an older base). Whatever main's link task cost the render
  every frame on silicon, the emulator charged none of it.
- **Per-request cost.** At a fixed ~7.2 requests/s (two runs per image, a
  pause tuned per image), silicon loses 3.53 % of idle frame rate on main
  and 6.78 % on the branch: **0.49 vs 0.93 % per (request/s)**. The branch
  still renders more frames under requests (30.94 vs 29.77 fps), but its
  idle gain is mostly spent by ~7 requests/s. The emulator cannot see either
  number at 447 fps.
- **Request latency in frames.** The emulator had `quad-wire-oracle`'s
  latency UART-bound (link RTT ≈ 1.8 frames); on silicon the same RTT is
  0.12 frame and main's request waits on the render thread instead (link RTT
  26 ms). The branch's 1.05-frame p50 at ~12.6 requests/s (0.87 frame at
  ~7/s) is a silicon number the emulator could not have predicted.

**Root cause (not established)** — `lp-emu-esp32v3` has one time grade,
`t1`, with no memory-cost model (the walk record's §3 already shows `t1`
charging 18 cycles for a `cycle-probe` sample silicon takes 2,910 for, cold,
and a compile at 0.26× silicon's time). A render whose `.text` runs from
flash through the cache is the likeliest place a 14× gap comes from, as on
the C6 (2026-10-01 entry, the inverse sign) — but nobody has profiled the
classic's frame on silicon, and the RMT/wire pusher's own waits are not
ruled out.

**What it means now** — memory figures from this machine still transfer
(PR #943's heap delta was byte-exact on silicon: +3,456 B used, both reply
shapes); frame-relative timing does not, in either direction. A classic
latency or frame-rate claim needs a desk number until `t2` exists.

**Fix (open)** — the classic's named future `t2`/`t3` (walk record §3), with
a flash-cache cost calibrated on silicon the way the C6's `t3` was. Until
then, plans should not state a classic frame-rate effect from the emulator.

Silicon numbers: agent-run desk sitting 2026-10-03, not graded by a
transcript. Emulated numbers: `configuration=lp-emu:esp32v3:t1`, lp-emu
`c042102d9` (as built into `lp-cli` at main `d68791d96`).
