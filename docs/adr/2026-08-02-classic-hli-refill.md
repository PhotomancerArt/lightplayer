# ADR: Classic-ESP32 level-4 RMT refill (the HLI experiment)

- **Status:** Rejected — measured and parked (G2, 2026-08-02)
- **Date:** 2026-08-02
- **Deciders:** Yona (G2, 2026-08-02); authored overnight per explicit
  authorization
- **Supersedes:** None (P5 of plan `2026-08-01-1459-rmt-priority-hli` was
  recorded NO-GO; this is that phase reopened as a droppable experiment on its
  own branch)
- **Related:** `docs/adr/2026-07-29-license-provenance-discipline.md`,
  `docs/adr/2026-07-31-lp-ws281x-multi-channel-driver-adoption.md`

## Outcome (G2, 2026-08-02)

**Parked, not shipped.** The success bar (scan truncation to ~S3-chip levels)
was not met; the level-4 path measured 2–3× worse than level-3 under scan
(reproduced, unexplained after one instrumented re-measure — confounds and the
de-censoring follow-up are documented above); and the radio-off wins solve
nothing measurably broken radio-off. The implementation — vector, `lp-ws281x-hli`
contract crate, host suite, stress harness — is preserved unmerged on branch
`claude/classic-hli-experiment` (PR #272, closed as parked). Reopen conditions
live in the plan's P5 file: radio landing on the classic (RAM diet first), the
start-path masking fix in Rust getting first shot, and — should truncation
still matter then — the interleaved-cells + de-censored-instrument experiment
before any further level-4 work.

## Context — the whole chain, honestly

This decision record exists as much for its provenance story as for its
technical one. In order:

1. **The starvation problem.** The classic ESP32's WS281x output refills RMT
   RAM from an interrupt with a hard deadline (~80 µs at 64-word halves).
   Historically, WiFi scan load truncated 69 % of frames on this chip; the C6
   measured 28 % under the same class of load at `Priority::max()` — and P4 of
   the parent plan attributed the loss to **entry delay** (interrupt-to-service
   latency during the radio's masked windows), not to refill work. On RISC-V
   there is nothing above the maskable levels; on Xtensa, high-priority
   interrupt levels 4–7 exist above `EXCM_LEVEL = 3` — the classical "HLI"
   escape. P5 (a clean-room level-4/5 vector) was specified, then recorded
   NO-GO on the measured basis that the radio-off classic trips nothing and
   radio cannot yet boot beside the server (M6's RAM wall).

2. **The license question that reopened it.** A community tip (source
   deliberately not named) noted that WLED — whose WiFi-stable LED output is
   the folk-standard existence proof — was **MIT-licensed until 2024-10-15**,
   when it relicensed to EUPL (WLED PR #4194). Verified from repository
   history: the relicense is not retroactive, pre-switch revisions remain MIT,
   and the maintainer explicitly confirmed pre-switch MIT permanence in that
   PR. That opened a possible port-with-attribution path where our rules had
   assumed only copyleft.

3. **The firewalled provenance investigation and its verdict.** Under the
   protocol Yona authorized, a firewalled agent — and only that agent —
   examined WLED source at the last MIT revision (`44e28f9`, tag v0.15.0-b6)
   to answer "is there an HLI shim, and is it WLED's own work?" The verdict,
   facts only, no code crossing the firewall: **WLED contains no HLI shim at
   any revision. It never did.** Zero assembly files, zero interrupt-priority
   manipulation, zero RMT/I2S peripheral code in the repo. WLED's WiFi-stable
   output is implemented entirely in **NeoPixelBus (Makuna), LGPL-3.0** —
   primarily I2S/DMA on the classic ESP32 — selected by WLED's dispatch
   headers. The "WLED HLI shim" our findings had referenced is a myth; the
   thing that actually exists is LGPL and absolutely off-limits.
   **Contamination log:** only the firewalled provenance agent read WLED
   source; no implementation detail crossed into the planning session or any
   implementer; the clean-room path remained intact and provably so. Neither
   WLED nor NeoPixelBus was opened during this implementation.

4. **Therefore: clean-room from Espressif's Apache-licensed material** — which
   is what the plan said before the detour. Implementation references, all
   permissive or primary: esp-idf's `hli_vectors.S`/`hli_api` (Apache-2.0;
   behavioral precedent for a level-4 vector coexisting with an RTOS),
   `xtensa-lx-rt` and `esp-hal` (MIT/Apache-2.0; the vector-entry contract and
   interrupt allocator this must coexist with), the Xtensa ISA Reference
   Manual, the ESP32 TRM, and the `esp32` PAC field docs. Every new file
   carries a provenance header and the clean-room statement.

## Decision (proposed)

Service the classic's RMT `tx_thr_event` from a hand-written **level-4**
Xtensa vector, behind cargo feature `hli_refill` (default OFF; the shipping
image is untouched — verified byte-identical section sizes), structured as:

- **`lp-ws281x-hli`** (new host-tested crate): the `repr(C)` state contract
  and a pure-Rust reference model of the handler's algorithm, pinned against
  `lp-ws281x`'s driver as oracle (byte-identical wire streams). The firmware
  derives every assembly offset from these structs via `offset_of!` and runs
  the model on the thread-side start path, so the assembly's spec executes on
  silicon every frame.
- **`fw-esp32v3::output::rmt::hli`**: the vector (`global_asm!`, call0
  discipline, no windowed instructions, a2..a13+SAR to a static DRAM save
  area, IRAM code + literals, ack-all storm guard, `rfi 4`) and a
  `shared_driver`-shaped thread side, so the endpoint layer swaps refill paths
  without changing anything else. `lp-ws281x`'s portable core is untouched.

### Level choice: 4, not 5

Level 4 is the lowest level above `EXCM_LEVEL` (3). On the ESP32 the level-4
CPU interrupts are {24, 25, 28, 30} (xtensa-lx-rt `XCHAL_INTLEVEL4_MASK` =
`0x5300_0000`); 24/25 are level-triggered externals, 28/30 edge. Level 5
offers only interrupt 26 (16 is the internal CCOMPARE2, 31 is edge) and buys
**nothing**: in the esp-hal ecosystem, `esp-sync`'s `SingleCoreInterruptLock`
— the implementation under `critical_section::with`, esp-radio's
`wifi_int_disable`, and esp-storage's flash windows — executes **`rsil 5`**,
masking levels 4 *and* 5 alike. (esp-idf's equivalent stops at
`EXCM_LEVEL = 3`, which is why HLI folklore from that world promises more
than this world can deliver.) So: **CPU interrupt 24, level 4** — the
conventional HLI level, level-triggered, unclaimed.

### Coexistence contract with esp-hal / esp-rtos

- **Hook:** override `__naked_level_4_interrupt`, the linker `PROVIDE` seam
  xtensa-lx-rt publishes for exactly this purpose. No vector table is
  stomped; the level-1..3 machinery, esp-rtos scheduling and esp-radio
  allocation are untouched.
- **Routing:** the RMT source's own interrupt-matrix map register
  (`DPORT core_0_intr_map[RMT]`) is written to 24 — one register per source,
  so this simultaneously un-maps it from esp-hal's level-3 path. esp-hal's
  allocator never assigns CPU interrupts above 23.
- **INTENABLE** is read-modify-written via `xtensa_lx::interrupt::enable_mask`
  — the same primitive esp-hal uses; neither side clobbers the other.
- **What still masks level 4** (measured boundary, not a defect): `rsil 5`
  critical sections (esp-sync, everywhere), and the rare INTENABLE=0 windows
  (`xtensa_lx::interrupt::free` — esp-hal's classic UART sync path; the panic
  handler). What level 4 escapes: every `rsil ≤ 3` lock (embassy /
  `PriorityLock`), and all time spent executing level ≤ 3 handlers including
  the level-3 RMT dispatch itself.
- **Handler discipline:** no calls, no window traffic, no flash-mapped
  reads, nothing that can fault; every touched register restored; level 4
  cannot nest with itself. The interruptee's stack is never used.

### An algorithmic consequence unique to level 4

The level-3 driver plants its guard word (flicker protection) before each
refill, relying on entry latency to have moved the reader past the guard
slot. At level 4 the entry delay is **zero** — the reader is routinely still
*on* the slot — so a pre-fill-only guard degenerates to no protection at all
(measured: `skips == refills` before the fix). The handler (and model) now
retry the guard after the fill; `skips` counts only refills left unguarded by
both attempts, and measured skips returned to 0.

## Measured results (desk DOM-Z-102, classic ESP32 v3.1)

Feature-off regression (server + telemetry, quad-strips-v3 = 4×30 LEDs,
radio-off): identical to the P4 baseline — 3,932 frames / 180 s, 0 trips,
lag 15.0/17, ch0 entry_max 55 with the per-frame two-bucket fingerprint.

Feature-on, radio-off (same image shape, `hli_refill` added):

| metric (per channel, 180 s) | level-3 baseline | level-4 |
|---|---|---|
| frames / trips | 3,932 / 0 | 3,915 / 0 |
| refill lag avg / max (words) | 15.0 / 17 | 3.0 / 3 |
| entry_max (ch0 / others) | **55** / 8 | **0** / 0–1 |
| entry delays beyond bucket 0 | ~2 per frame on ch0 | **zero in 43 k refills** |
| guard skips | 0 | 0 (post-fill retry) |

The ch0 start-path masking that consumed up to 69 µs of the 80 µs deadline
once per frame under level 3 is **entirely absent** at level 4 — which
classifies it as `rsil ≤ 3`-class masking, and confirms P4's "fixable"
reading by construction rather than by patching the start path.

Radio-linked head-to-head (`hli_stress` harness: same boot, same board,
4×30 LEDs at the baseline ~21 fps pace, WiFi scan cells ≥150 s, level-3
cells then a live switch to level 4):

Two full runs (run3, and run4 with the selection-mismatch instrument);
per-cell deltas from the cumulative telemetry. Cell order within each boot:
L3 idle → L3 scan → live switch → L4 idle → L4 scan.

| cell (run4) | frames/ch | trips (worst ch) | trips (all ch) | entry_max | lag_max | selmis |
|---|---|---|---|---|---|---|
| L3, radio idle | 2,267 | 0 (0 %) | 0/0/0/0 | 9 | 20 | n/a |
| L3, S2 scan | 2,780 | 39 (1.40 %) | 28/4/39/24 | 57 | 26 | n/a |
| L4, radio idle | 2,269 | 0 (0 %) | 0/0/0/0 | 2 | 3 | 0 |
| L4, S2 scan | 2,792 | 78 (2.79 %) | 78/22/72/47 | 49 | 4 | 0 |

(run3, uninstrumented, same shape: L3 scan 21–30 trips/ch ≈ 0.8–1.1 %;
L4 scan 49–87 ≈ 1.8–3.1 %.)

**The honest reading.**

1. Both paths are orders of magnitude below the historical 69 % (the harness
   drives channels sequentially at the app path's own pace, each with the
   full 80 µs deadline — the shipping-shaped load, not the experiment repo's
   worst case). The classic under scan is a ~1–3 % problem in this shape,
   on either path.
2. **The success bar ("truncation to ~S3-chip levels, ~1 %") was not met by
   the level-4 path — and, reproduced across two runs, the level-4 cells
   truncated 2–3× MORE than the level-3 cells**, despite strictly lighter
   recorded entry-delay tails (max 49 vs 57, near-empty upper buckets) and
   12× lower refill lag.
3. Why the entry histograms cannot explain the trips, on either path: a
   refill delayed past the deadline ends the frame via the guard, and the
   handler's end-beats-threshold precedence (both paths, by design) then
   swallows the late event — its delay is never recorded. The entry-delay
   instrument is **censored at exactly the deadline**, so trips measure the
   over-deadline outage population that the histograms structurally miss.
   Those outages are `rsil 5`-class (nothing below NMI crosses them), which
   is consistent with level 4 not helping — but not with it measuring
   *worse*.
4. The level-4 excess is **unexplained after one instrumented re-measure**
   (the selection-mismatch counter came back zero: no missed/duplicated
   event bookkeeping at any serviced entry). Known confound: cell order —
   the level-4 scan cell always ran ~5 minutes later in the boot, and scan
   load (AP environment, scan timing) was not controlled across cells. The
   next experiment, if G2 wants one, is interleaved or order-reversed
   cells, plus a trip-adjacent delay capture (record the pre-end pending
   causes) to de-censor the instrument.
5. **Advisory recommendation to G2: park, do not ship.** The radio-off wins
   (entry 55 → 0, lag 15 → 3) are real but solve no problem the level-3
   path measurably has radio-off (zero trips either way); the radio case is
   the entire point, and there the level-4 path met neither the success bar
   nor parity. The vector, the contract crate, the host suite, the
   `rsil 5` ceiling finding and the censoring analysis all remain as
   assets for a future reopen — which is exactly what "droppable
   experiment" was designed to leave behind.

## RAM / flash cost (measured, telemetry build vs telemetry+hli)

- `.bss` **+816 B** (the four-channel bank, 768 B, + 64 B register save
  area) — the pre-staged-state cost the M6 ledger wants recorded.
- `.data` −1,088 B and `.rwtext` −452 B: the idle level-3 refill path and its
  IRAM trampoline drop out of the link. Net DRAM: **−272 B** (returned to
  `.stack`). Flash `.text`: +264 B.
- Per frame, transient: one wire-order staging `Vec` per channel (frame-sized,
  ~90 B at product load), reused across frames.

## The argued alternative: an I2S/DMA backend (what WLED actually does)

The architectural *idea* behind WLED's WiFi stability — uncopyrightable, and
the honest counterpoint this ADR must argue against — is not a faster
interrupt but **no deadline at all**: NeoPixelBus drives the classic's I2S
peripheral in LCD/parallel mode, pre-expanding each WS281x bit into 3–4 DMA
clock slots, so the wire is fed by DMA and the CPU races nothing. Sketch of
the tradeoffs on this chip:

- **RAM:** the DMA buffer holds the whole expanded frame: ≈ 4 bytes per LED
  byte single-lane (4×30 LEDs ≈ 1.4 KB), but the parallel-8 mode packs eight
  strips into the same expanded stream (≈ 2.9 KB for eight 30-LED lanes) —
  comparable to what four RMT windows + staging already cost, but it *scales
  with frame size*, which on a chip with ~9 KB of free heap at product load
  is the real constraint. Double-buffering for tear-free updates doubles it.
- **Peripheral budget:** classic has two I2S units; I2S0 is the one with the
  LCD mode NeoPixelBus uses. No current consumer in this firmware.
- **Structure:** an `lp-ws281x` backend below the same driver seam is
  plausible (the core is deliberately backend-agnostic), but latch timing,
  lane-count rigidity (all strips share one clock and one frame length) and
  the expansion step are a genuinely different driver, not a shim.
- **When it wins:** if the classic ever needs to be scan-proof *through*
  `rsil 5` windows — the one masking class level 4 cannot cross — DMA is the
  only software answer on this chip.

Recorded as the next lever beyond HLI, not as tonight's work.

## Consequences

- The experiment ships behind `hli_refill` / `hli_stress`, default off, on
  its own branch/PR — droppable by closing the PR, exactly as authorized.
- If accepted: the classic gains a refill path whose worst measured entry
  delay radio-off is 0–1 words (vs 55), with equal-or-better guard coverage,
  at +816 B `.bss` and a net DRAM *saving*. The S3 shares the ISA and the
  seam; adoption there is a separate decision (same `PROVIDE` hook exists).
- If parked: the measured numbers, the `rsil 5` ceiling finding, and the
  contract crate remain — a future reopen starts from a proven vector rather
  than a spec.
- Either way, the esp-sync `rsil 5` finding belongs upstream eventually: in
  the esp-hal world, *nothing* below NMI escapes `critical_section`, which
  makes every HLI folklore claim imported from esp-idf quietly wrong here.

## Alternatives considered

- **Port WLED's shim (MIT-era).** Dissolved by the provenance verdict: no
  such shim exists; the mechanism is NeoPixelBus (LGPL) I2S/DMA. Off-limits.
- **Level 5.** No benefit over 4 (`rsil 5` masks both), costs the last
  usable level-triggered high-priority slot. Rejected.
- **Asm-trampoline into constrained Rust at level 4** (esp-idf's hli_api
  shape). Requires a full register-file spill (~145+ cycles) plus a dedicated
  stack, to run code that must not allocate, lock, or touch flash anyway —
  P4's data says entry delay was the enemy, so the fixed-work pure-asm
  refill is both faster and easier to prove. Rejected for this experiment;
  the save-area design leaves room to grow one later.
- **Fix the start-path masking in Rust first** (the recorded reopen ladder's
  step 2). Still worth doing for the level-3 path; the experiment measured
  the masking's class from above instead of hunting it from below, and the
  level-4 path makes the question moot where enabled.
