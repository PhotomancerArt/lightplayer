# ESP32-C6 emulator: browser/phone bench baseline (M3)

2026-09-07. Plan `2026-09-07-0827-emu-speed-ladder`, milestone M3
(`m3-web-bench-rig.md`). Numbers below are at commit `0196623154`, one
branch tip past M1's landed commit `bc196fc5a` (PR #566) — M3 adds the rig
only, no emulator source changes (D6), so these figures are M1's release
overrides plus bookkeeping patch, measured through the browser instead of
natively.

**Status: desktop numbers below are final. The iPhone 16 Pro Max row is
filled in at the M3 review gate — this report is updated after that
gate, not before, per `docs/process/review-gates.md`.**

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

Desktop numbers below were taken in the Claude Code harness's Chromium
pane (UA reports Chrome 148, V8), which is a **visible** tab throughout —
relevant because Chromium was observed elsewhere in this plan's research to
run ~3x slower per instruction when its tab is hidden (`notes.md`,
"Phone baseline"); a browser walk-up gate should keep the tab foregrounded.

## Desktop Chrome (M2 Max, Chromium pane, visible tab)

Two full sequences, back to back, with the 1-minute load average quoted at
the time of each (`sysctl -n vm.loadavg`) — the desk had other agents
building throughout this session, so **only the ratios and the run-to-run
comparison are meaningful, not the absolute wall-clock number** (AGENTS.md
"never gate on emulated microseconds"; same caveat the native probe's
`rt(wall)` carries).

Run A, load 1m ≈ 16 at completion:

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

Run B, immediately after, load 1m ≈ 15:

| run | image | grade | wall s | instr/s | real time |
|---|---|---|---:|---:|---:|
| 1 | harness | t1 | 7.92 | 54.3 M | 0.35x |
| 2 | harness | t2 | 4.54 | 63.8 M | 0.62x |
| 3 | harness | t1 | 6.41 | 67.2 M | 0.44x |
| 4 | harness | t2 | 4.46 | 65.0 M | 0.63x |
| 5 | boot-idle-memfs | t1 | 0.56 | 56.3 M | 5.31x |
| 6 | boot-idle-memfs | t2 | 0.55 | 54.1 M | 5.49x |
| 7 | boot-idle-memfs | t1 | 0.57 | 56.2 M | 5.30x |
| 8 | boot-idle-memfs | t2 | 0.55 | 54.0 M | 5.47x |

Headline: **harness t2 ≈ 0.62–0.65x real time**, matching the 2026-09-07
08:40 spike's 0.61–0.62x closely (that run used the same opt-3 + bookkeeping
tree; the small delta is desk-load noise on the wall-clock-derived instr/s,
not a regression — the wasm module and reference images are unchanged
between the two measurements). `boot-idle-memfs` runs 5.3–5.6x real time, as
expected for a much lighter, mostly-idle image.

**Instruction counts are byte-identical to native**, the identity oracle
this rig is held to: `stopped after 430518352 instructions` (harness, t1),
`289953872` (t2), `31806244` / `29577878` (boot-idle-memfs t1/t2) — verified
against `scripts/emu/bench-c6.sh --no-build --no-promote` on the same
reference images at this commit (native probe output, load 1m ≈ 17–18):

```
harness t1: stopped after 448306890 cycles (2801918 us emulated, 430518352 instructions, grade lp-emu:esp32c6:t1)
harness t2: stopped after 448664403 cycles (2804152 us emulated, 289953872 instructions, grade lp-emu:esp32c6:t2)
boot-idle-memfs t1: stopped after 480000000 cycles (3000000 us emulated, 31806244 instructions, grade lp-emu:esp32c6:t1)
boot-idle-memfs t2: stopped after 480000000 cycles (3000000 us emulated, 29577878 instructions, grade lp-emu:esp32c6:t2)
```

**First-run tier-up penalty on Chrome** (run 1 vs run 3, same grade,
harness t1): Run A 54.8 M -> 69.5 M instr/s (+27%); Run B 54.3 M -> 67.2 M
(+24%). V8's Liftoff-to-TurboFan tier-up is visible but modest here — the
hot dispatch loop tiers up within the first couple of seconds of a ~7 s run,
consistent with the wasm research's finding that this module's compile +
instantiate cost is trivial (<50 ms) and the tax is in the interpreter
warming up, not module load.

**t1/t2 per-instruction gap in Chrome (visible tab):** t1 and t2 instr/s
land within noise of each other in both runs (harness: t1 54.8–69.5 M vs t2
63.8–66.9 M; boot-idle-memfs: t1 54.0–57.7 M vs t2 51.9–56.3 M) — **no
per-instruction gap** between grades when the tab is visible. This matches
the 2026-09-07 research note that Chromium's t1/t2 gap (3x, attributed to
MMIO dispatch cost) only appeared when the pane was **hidden**; this rig's
runs were all in a foregrounded tab.

## Desktop Safari

Not captured this session — the harness's Browser pane tool drives
Chromium only, and there is no macOS Safari automation path available to
this agent. `just bench-emu-web` serves on `localhost` as well as the LAN,
so a manual Safari run is one open-and-click away; recorded here as a gap,
not attempted by hand to avoid an unreviewed manual step in an automated
report. Follow-up filed (see Deviations).

## iPhone 16 Pro Max (Safari)

*Filled in at the M3 review gate — Yona runs `just bench-emu-web` on the
phone against the LAN URL; the director relays the four gate answers back
to this session, which updates this section and finishes M3.*

For context, the *previous, ad hoc* run of this same rig (2026-09-07 08:40,
before it was made repeatable) measured iPhone 16 Pro Max / Safari 26.6.1:
`t2` warm 76–79 M instr/s, **0.74–0.76x real time**; raw JSON in the
planning directory's `baselines/`. That number is cited for continuity, not
reused as this milestone's result — the gate re-measures on the committed
rig.

## Gap to the D1 target

D1 (`plan.md`): 1x real time at `t2` on an iPhone 16 Pro Max, proxied by
>=2x native / >=1x desktop Chrome on the harness. This baseline:

| proxy | measured | target | gap |
|---|---:|---:|---:|
| native M2 Max, harness t2 (M1's own probe, this commit) | 0.91–0.92x rt(user) | >=0.95x after M1 | ~on target (M1's follow-up: re-measure on an idle machine) |
| desktop Chrome, harness t2 (this report) | 0.62–0.65x | >=1x | **~1.5–1.6x more throughput needed** |
| iPhone 16 Pro Max, harness t2 | pending gate | 1x | 2026-09-07 08:40 ad hoc run: ~1.3x more needed |

The two 2026-09-07 baselines this milestone starts from: **0.76x phone**,
**0.62x Chrome** (both harness, t2, ad hoc pre-rig run). This report's
Chrome numbers (0.62–0.65x) land in the same band; M5 (block cache) is
where the plan expects the gap to close.

## Deviations

- Desktop Safari not measured — no Safari automation available to this
  agent; the rig serves on `localhost` so it is a manual one-open away.
- The `--collect` table's "device" column is a UA substring guess
  (iPhone/iPad/Mac/Android/other), not authoritative; good enough to sort
  a handful of uploaded results.
