# ESP32-C6 emulator: browser/phone bench baseline (M3)

2026-09-07. Plan `2026-09-07-0827-emu-speed-ladder`, milestone M3
(`m3-web-bench-rig.md`). Numbers below are at commit `992849de4`, the M3
rig-and-docs commit on top of M1's landed commit `bc196fc5a` (PR #566) — M3
adds the rig only, no emulator source changes (D6), so these figures are
M1's release overrides plus bookkeeping patch, measured through the browser
instead of natively.

**Status: final.** Gate held 2026-09-07; Yona ran two full sequences on an
iPhone 16 Pro Max against the LAN URL this rig printed. Both uploaded
results are in this report and in `target/emu-bench-web/result-*.json` on
the machine that served them.

## Method

`just bench-emu-web` builds the wasip1 module of `lp-emu-esp32c6`
(`CARGO_TARGET_WASM32_WASIP1_RUSTFLAGS="-C target-feature=+bulk-memory,+simd128,+nontrapping-fptoint"`,
release profile — the same per-crate `opt-level = 3` overrides M1 shipped
apply automatically since they key on package name, not target triple),
stages it beside the two pinned reference images `bench-emu-c6` uses
(`harness` = the compile-stress image, `boot-idle-memfs` = the idle/boot
image), and serves the page + a dedicated-Worker runner on the LAN. The
module is the **unmodified CLI binary** run under a ~150-line JavaScript
WASI preview1 shim implementing exactly the 15 imports it declares — zero
emulator source changes (D6).

The page runs each image x grade x repeat (`t1,t2,t1,t2` per image, in
manifest order) in the Worker, shows a live table, and uploads a
`result-<timestamp>.json` to the serving Mac on completion.
`scripts/emu/bench-web.sh --collect` reads every uploaded result and
prints device, engine guess (from the UA), image, grade, wall seconds,
instr/s and the real-time ratio (`emulated us / wall ms`).

## iPhone 16 Pro Max (iOS 18.7, Safari 26.6.1, JavaScriptCore)

Two full sequences, uploaded 21 minutes apart, each the full
images x grades x 2 repeats (8 runs). Raw JSON:
`result-2026-09-07T23-24-08-664Z.json` (seq 1) and
`result-2026-09-07T23-45-10-749Z.json` (seq 2).

Sequence 1 (23:24 UTC):

| run | image | grade | wall s | instr/s | real time |
|---|---|---|---:|---:|---:|
| 1 | harness | t1 | 5.30 | 81.2 M | 0.53x |
| 2 | harness | t2 | 3.89 | 74.5 M | 0.72x |
| 3 | harness | t1 | 5.92 | 72.7 M | 0.47x |
| 4 | harness | t2 | 4.16 | 69.7 M | 0.67x |
| 5 | boot-idle-memfs | t1 | 0.48 | 66.1 M | 6.24x |
| 6 | boot-idle-memfs | t2 | 0.46 | 64.7 M | 6.56x |
| 7 | boot-idle-memfs | t1 | 0.48 | 66.3 M | 6.25x |
| 8 | boot-idle-memfs | t2 | 0.46 | 64.2 M | 6.51x |

Sequence 2 (23:45 UTC, ~21 min later):

| run | image | grade | wall s | instr/s | real time |
|---|---|---|---:|---:|---:|
| 1 | harness | t1 | 5.70 | 75.6 M | 0.49x |
| 2 | harness | t2 | 3.77 | 76.8 M | 0.74x |
| 3 | harness | t1 | 5.73 | 75.1 M | 0.49x |
| 4 | harness | t2 | 3.91 | 74.1 M | 0.72x |
| 5 | boot-idle-memfs | t1 | 0.48 | 66.5 M | 6.28x |
| 6 | boot-idle-memfs | t2 | 0.46 | 64.6 M | 6.55x |
| 7 | boot-idle-memfs | t1 | 0.48 | 65.9 M | 6.21x |
| 8 | boot-idle-memfs | t2 | 0.46 | 64.9 M | 6.58x |

Instruction counts (430,518,352 / 289,953,872 harness t1/t2;
31,806,244 / 29,577,878 boot-idle-memfs t1/t2) are byte-identical to the
native probe and to the desktop Chrome runs below — the identity oracle
this rig is held to.

## Desktop Chrome (M2 Max, Chromium pane, visible tab)

Two full sequences, back to back, with the 1-minute load average quoted at
the time of each (`sysctl -n vm.loadavg`) — the desk had other agents
building throughout this session, so **only the ratios and the run-to-run
comparison are meaningful, not the absolute wall-clock number** (AGENTS.md
"never gate on emulated microseconds").

Run A, load 1m ~= 16:

| run | image | grade | wall s | instr/s | real time |
|---|---|---|---:|---:|---:|
| 1 | harness | t1 | 7.86 | 54.8 M | 0.36x |
| 2 | harness | t2 | 4.36 | 66.4 M | 0.64x |
| 3 | harness | t1 | 6.19 | 69.5 M | 0.45x |
| 4 | harness | t2 | 4.34 | 66.9 M | 0.65x |
| 5 | boot-idle-memfs | t1 | 0.59 | 54.0 M | 5.09x |
| 6 | boot-idle-memfs | t2 | 0.53 | 55.5 M | 5.63x |
| 7 | boot-idle-memfs | t1 | 0.55 | 57.7 M | 5.44x |
| 8 | boot-idle-memfs | t2 | 0.57 | 51.9 M | 5.27x |

Run B, load 1m ~= 88 (a much busier desk than run A — quoted per run,
not averaged):

| run | image | grade | wall s | instr/s | real time |
|---|---|---|---:|---:|---:|
| 1 | harness | t1 | 8.04 | 53.5 M | 0.35x |
| 2 | harness | t2 | 4.55 | 63.7 M | 0.62x |
| 3 | harness | t1 | 6.57 | 65.6 M | 0.43x |
| 4 | harness | t2 | 4.58 | 63.3 M | 0.61x |
| 5 | boot-idle-memfs | t1 | 0.60 | 53.1 M | 5.01x |
| 6 | boot-idle-memfs | t2 | 0.58 | 51.4 M | 5.21x |
| 7 | boot-idle-memfs | t1 | 0.59 | 53.7 M | 5.07x |
| 8 | boot-idle-memfs | t2 | 0.56 | 52.4 M | 5.31x |

Headline: **harness t2 ~= 0.61-0.65x real time** on desktop Chrome,
matching the 2026-09-07 08:40 spike's 0.61-0.62x closely — the wasm module
and reference images are unchanged between the two measurements; the small
delta is desk-load noise on the wall-clock-derived instr/s, not a
regression.

**Instruction counts are byte-identical to native**: verified against
`scripts/emu/bench-c6.sh --no-build --no-promote` on the same reference
images at this commit (native probe output, load 1m ~= 17-18):

```
harness t1: stopped after 448306890 cycles (2801918 us emulated, 430518352 instructions, grade lp-emu:esp32c6:t1)
harness t2: stopped after 448664403 cycles (2804152 us emulated, 289953872 instructions, grade lp-emu:esp32c6:t2)
boot-idle-memfs t1: stopped after 480000000 cycles (3000000 us emulated, 31806244 instructions, grade lp-emu:esp32c6:t1)
boot-idle-memfs t2: stopped after 480000000 cycles (3000000 us emulated, 29577878 instructions, grade lp-emu:esp32c6:t2)
```

## Desktop Safari

Not measured — the harness's Browser pane tool drives Chromium only, and
there is no macOS Safari automation path available to this agent.
`just bench-emu-web` serves on `localhost` as well as the LAN, so a manual
Safari run is one open-and-click away; not attempted this session to avoid
an unreviewed manual step in an automated report.

## The four gate questions, answered

**1. Warm `t2` real-time ratio on the iPhone 16 Pro Max, harness and
boot-idle.** Harness: 0.72x / 0.67x (sequence 1), 0.74x / 0.72x (sequence
2) — **~0.7x** across both sequences (69.7-76.8 M instr/s). `t1` on
harness ran 0.47-0.53x. Boot-idle-memfs `t2` ran **~6.5x** real time
(64.2-64.9 M instr/s) — the same instruction throughput as harness `t2`,
just a far lighter image relative to its own emulated timeout.

**2. First-run tier-up penalty (run 1 vs run 3) on Safari.** None visible.
In sequence 1, run 1 (harness t1, 81.2 M instr/s) was actually the
*fastest* of the two t1 passes — run 3 read 72.7 M, 10% slower, not
faster. JavaScriptCore tiers up within the first seconds of the run (the
harness image runs 5-6 s at t1), so by the time the page's timer starts
measuring wall clock the interpreter is often already warm. This is the
opposite of a first-run penalty and is consistent with the module's
trivial compile+instantiate cost (a few ms) reported in the wasm research.

**3. Does Safari show the t1/t2 per-instruction gap (MMIO dispatch cost in
wasm)?** No, not like Chromium's hidden-pane 3x gap. Sequence 1: 81.2 -> 74.5
M instr/s (t1 -> t2, harness), an 8% gap. Sequence 2: 75.6 -> 76.8 M, i.e.
`t2` ran *faster* than `t1` — within run-to-run noise. Safari's tab was
foregrounded throughout (matching the desktop Chrome runs, which also
showed no gap when visible) — the MMIO-dispatch-cost gap Chromium showed
was specifically a hidden-tab effect, and neither browser shows it
foregrounded.

**4. Any thermal drop across repeats (run the sequence twice back to
back)?** No. Sequence 2's harness `t2` (0.74x / 0.72x) matches or slightly
beats sequence 1's (0.72x / 0.67x) 21 minutes later. Within a sequence the
spread is about +-5%; sequence 1's second harness `t1` pass ran 7-10%
slower than its first, but that pattern did not recur in sequence 2 (both
`t1` passes read within 1% of each other) — read as run-to-run scheduling
noise on a 4-core phone, not a thermal trend across sequences.

## Gap to the D1 target

D1 (`plan.md`): 1x real time at `t2` on an iPhone 16 Pro Max, proxied by
>=2x native / >=1x desktop Chrome on the harness, both after later
milestones (M5's block cache). This baseline, at the M1 commit:

| proxy | measured | target | notes |
|---|---:|---:|---|
| iPhone 16 Pro Max, harness t2 | ~0.7x (0.67-0.74x) | 1x, after M5 | **~1.35-1.45x more throughput needed** |
| desktop Chrome, harness t2 | 0.61-0.65x | >=1x, after M5 | quoted at load 16 and load 88; ratio stable across both |
| native M2 Max, harness t2 (M1's own probe) | 0.91-0.92x rt(user) | >=0.95x after M1 | close; M1's follow-up flagged a quiet-machine re-measurement as still owed |
| desktop Safari | not measured | >=1x, after M5 | no automation path this session; manual run available at `localhost` |

The two 2026-09-07 08:40 ad hoc baselines this milestone starts from —
**0.76x phone**, **0.62x Chrome** (both harness, `t2`, pre-rig, same opt-3 +
bookkeeping tree) — sit inside the noise band of this report's repeatable
numbers (phone 0.67-0.74x, Chrome 0.61-0.65x). The phone's gap to D1 (1x)
lands at **~1.35-1.45x** more throughput needed on the harness image; M5
(block cache) is where the plan expects that gap to close.

## Deviations

- Desktop Safari not measured — no Safari automation available to this
  agent; the rig serves on `localhost` so it is a manual one-open away.
- The `--collect` table's "device" column is a UA substring guess
  (iPhone/iPad/Mac/Android/other), not authoritative; good enough to sort
  a handful of uploaded results.
- A background-task teardown killed the serving `miniserve` process once
  mid-gate (unrelated to the rig itself); restarted detached
  (`nohup ... & disown`) so it survived the rest of the gate.
