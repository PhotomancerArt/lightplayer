---
status: open           # recorded, not diagnosed and not fixed
found: 2026-09-08      # how: ci (Emulator C6 (x64), run 34177006492, PR #585)
area: lp-emu/esp/lp-emu-esp32c6/tests/usb_control.rs (G3-1 :195, G3-1b :344);
      mechanism reaches lp-fw/fw-esp32-common/src/server_loop.rs (heartbeat cadence)
class: unenforced-test-precondition
related:
  - 2026-09-08-cold-target-dir-links-esp-hals-stock-rodata.md
  - 2026-09-08-the-roms-usb-console-drops-what-the-drain-latency-delays.md
  - 2026-08-05-cross-core-panic-races-the-isr-thread.md
  - lp2025/2026-09-06-1001-esp-emulator/m6-honest-usb-serial-jtag.md
---
# The M6 USB gates pin a heartbeat's exact millisecond, which the firmware never promises

**Symptom** — `Emulator C6 (x64)` failed once on PR #585 (the emulator speed
ladder's M2), head `e1e1f3b9e`, [run
34177006492](https://github.com/PhotomancerArt/lightplayer/actions/runs/34177006492),
job 101908362651. **Two** tests in `tests/usb_control.rs` failed, both on a
heartbeat *string*:

```text
thread 'g3_1_the_cable_comes_out_at_six_seconds_and_the_link_comes_back_at_nine' panicked at
  lp-emu/esp/lp-emu-esp32c6/tests/usb_control.rs:195:10:
the 5 s heartbeat reached the host
thread 'g3_1b_a_port_held_closed_after_the_replug_holds_a_packet_until_it_opens' panicked at
  lp-emu/esp/lp-emu-esp32c6/tests/usb_control.rs:344:10:
no heartbeat after the recovery
test result: FAILED. 2 passed; 2 failed
```

Both are `delivered.find(…).expect(…)` on an exact substring —
`"uptime_ms":5000` at :195, `"uptime_ms":15000` at :344 — over the bytes the
emulated USB-Serial-JTAG host received. Neither assertion involves wall-clock
time: the run is 12 s of *emulated* time under `TimeGrade::T1`, stopped by
cycle count.

The other two tests in the same binary passed, and one of them is the
determinism gate: **G3-2 runs G3-1's exact script twice in the same process and
asserts byte-identical delivered logs**, identical cycles, instructions and idle
skips. It passed on the failing run. So the emulation was self-consistent
*within* that process; what was missing was the string.

**What the four neighbouring CI runs say.** G3-2 prints its digest every run,
which makes the delivered log comparable across runs:

| CI run | head | emu-c6 | G3-2 delivered | sha256 | idle skips |
| --- | --- | --- | --- | --- | --- |
| 34173296218 | `469048b19` | ok | 3569 B | `5c29615f48…` | 24,706 |
| 34174911311 | `de754622c` | ok | 3569 B | `4255775f48…` | 24,197 |
| **34177006492** | **`e1e1f3b9e`** | **FAIL** | **3587 B** | **`be1b38763f…`** | **24,566** |
| 34178163836 | `f23e97847` | ok | 3569 B | `a271116337…` | 24,858 |
| 34200662366 | `140ac6327` (main) | ok | **3628 B** | `a4f43282ef…` | 23,812 |

Read it carefully — each row is a **different tree**, so the differences are not
run-to-run noise on one input. What the table does establish:

- The failing run's delivered log was **18 bytes longer**, not short by a frame.
  A heartbeat frame on this link is 512 bytes in the committed silicon capture
  (`lp-emu/transcripts/esp32c6/boot-idle-flash/silicon-esp32c6-2026-09-07-735af98ae.txt`),
  so an *undelivered* heartbeat would show as a large negative delta. The link
  was not starved; the search string was absent from a log that was, if
  anything, fuller.
- The digest differs on **every** run. The delivered bytes carry per-build
  content (memory figures, identity), so a moving digest is expected.
  **Neither the digest nor the length is a cross-run invariant** — the fifth row
  is a *passing* run at 3628 B, longer than the failing run's 3587 B. So "18
  bytes longer" carries no significance on its own; what survives is only the
  bound above, that no run is short by anything like a 512 B frame.

**Correcting one thing about how this was reported.** The failure was *not*
cleared by a rerun of the same tree. Run 34177006492 is the **only** CI run
`e1e1f3b9e` ever had. The run that passed next, 34178163836, is head
`f23e97847` — `e1e1f3b9e` **plus a merge of main**, which brought in M5 P2's pin
fabric and the GPIO/RMT emulator changes (`periph/gpio.rs` new, `periph/rmt.rs`,
`periph/accept.rs`, `regs/output_signals.rs`). Those are inputs the emulated run
depends on directly. So "a rerun cleared it" is unestablished: nobody has re-run
the failing tree. The 22-suite local `just test-emu-c6` pass taken before and
immediately after that CI run was on the failing tree, but on a different
machine and a different firmware build — which is the comparison this entry is
about, not a rerun of it.

**The leading hypothesis — the assertion pins a value nothing pins.**
`run_server_loop` seeds two clocks from two separate calls and derives the
reported uptime from one while gating the send on the other
(`lp-fw/fw-esp32-common/src/server_loop.rs`):

```rust
let mut heartbeat_last_sent = time_provider.now_ms();
let startup_time = time_provider.now_ms();
…
if current_time.saturating_sub(heartbeat_last_sent) >= HEARTBEAT_INTERVAL_MS {
    …
    uptime_ms: current_time.saturating_sub(startup_time),
    …
    heartbeat_last_sent = current_time;   // re-seeded, so drift accumulates
}
```

`uptime_ms` is therefore *whichever millisecond the server loop happened to
sample first at or after the 5 s boundary*, minus a start stamp taken one
statement later than the one the boundary is measured from. It equals exactly
5000 only while the loop samples the clock at least once per millisecond across
that boundary — margin the idle guest currently has and nothing guarantees. One
slower frame at the wrong moment yields `"uptime_ms":5001` and the `find` fails.
The re-seed makes it worse downstream: the 10 s and 15 s beats are 5000 ms after
their *predecessor's* sample, so any drift is carried, and G3-1b's
`"uptime_ms":15000` needs three consecutive clean landings.

**Silicon lands on the millisecond too, which is why nobody noticed.** The
committed `boot-idle-flash` capture from a real C6 carries `uptime_ms":5000`,
`:10000` and `:15000` — the same three exact values. So this is not the emulator
being sloppier than hardware; it is the same margin on both, and the gates were
written against a property that has simply always held. That is what makes it a
precondition rather than a bug in either: the guest samples its clock often
enough at idle that the first sample past the boundary *is* the boundary, and
nothing in the firmware, the emulator, or the test says it must be.

The mechanism above is a hypothesis. It explains why the log was longer rather
than shorter, why a byte-identical pair of runs inside the failing process is
compatible with the failure, and why the two failures were the two
exact-millisecond assertions — but nobody has yet printed the uptime values the
failing run actually delivered.

**The image half of this stopped being a hypothesis hours after this entry was
filed.** It was suggested at filing time that the runner's firmware image
differs from a local build because the `LP_EMU_BUILD_FW=1` path is not
reproducible, and this entry recorded that as untested. It is now filed,
measured and fixed as its own defect —
[`2026-09-08-cold-target-dir-links-esp-hals-stock-rodata`](2026-09-08-cold-target-dir-links-esp-hals-stock-rodata.md)
(PR #601, merged `d2f38170b`): `fw-esp32c6/build.rs` patched esp-hal's
`rodata.x` by *scanning* for a directory cargo had not been told to write
first, and returned quietly when it was absent, so **build 1 of a cold target
dir linked esp-hal's stock four-section layout and build 2 the merged one**. A
CI tree is always cold. Every run in the table above therefore ran a firmware
image with a different rodata layout from any warm local build — including the
local `just test-emu-c6` passes this entry compares against.

That is precisely the input the mechanism above needs: rodata layout moves code
and data placement, placement moves the guest's instruction timing, and guest
timing is what decides which millisecond the server loop samples first past the
5 s boundary. The two halves are one story, not competing explanations.

**What is still open is whether it explains the *intermittency*.** The two
sources disagree in a way worth resolving rather than papering over: that
entry's symptom section says a cold tree deterministically gets build 1, while
PR #601's own title and its first commit say cargo "ordered nothing, so on a
cold tree the guess **could** run first and find nothing" — an ordering hazard
in a parallel, load-dependent build-script schedule, which would vary run to
run. Deterministic-on-cold cannot produce a one-off; racy-on-cold can. Deciding
which it was is now the sharpest question here, and it is answerable by reading
that build's ordering rather than by re-running anything.

**Everything above is era-bound, and the era ended at `d2f38170b`.** Since that
commit the `Emulator C6 (x64)` job builds a *different image* than the one every
row of the table ran. A recurrence after it is new evidence about a new image,
and the byte counts here are not comparable to it.

**Regression coverage** — none, and that is the point of this entry: the flake
is recorded, not fixed, so the next person to hit it starts from the evidence
above instead of re-deriving it.

**What would settle it, cheaply, in this order.**

1. Re-run `e1e1f3b9e` on a runner (`gh run rerun`, or push the tree under a
   scratch branch). Same tree, same job: a second failure moves this from
   "flake" to "that tree fails on the runner", which is a different and much
   easier bug.
2. Print what actually arrived. The tests already hold `delivered`; a
   failure-path dump of every `uptime_ms` substring in it turns "the string is
   missing" into "the string is 5001", which decides the whole entry in one run.
3. ~~Compare images between runner and local for the same tree.~~ **Done, from
   the other side, by PR #601** — they differed, and it has its own entry. What
   is left of this step is the narrower question above: was the cold-tree
   ordering a race or a certainty.

**The trap for whoever fixes this.** Do not relax the exact strings globally.
G3-1b's next assertion is a **negative** one —

```rust
assert!(!delivered.contains("\"uptime_ms\":10000"),
    "the heartbeat written into a closed port arrived whole, which would mean the \
     committed endpoint took bytes it had no room for");
```

— and it is load-bearing: it is how the test proves the held packet was
truncated. Under exactly the drift hypothesized above, that assertion passes
**vacuously**, because a beat reported as `10001` is not the string it is
looking for either. Loosening the positives to a pattern without tightening this
negative to the same pattern leaves a test that can only pass. That is the
silent half this class is named for.

**Lesson** — an assertion can be wall-clock-independent and still be a timing
assertion. These gates are about *delivery* — did the byte cross the link, in
which window — but they identify the payload by a field whose value is a
*sample* of the guest's own clock, so a delivery gate silently acquired a
scheduling precondition it never establishes. The shape to reach for when
matching a periodic frame is to match on what the gate is actually about (a
heartbeat arrived, in this window, in this order) and to bound the timestamp
rather than pin it — and, if an exact value is genuinely wanted, to make the
firmware emit an exact one rather than to hope the sampler lands.
