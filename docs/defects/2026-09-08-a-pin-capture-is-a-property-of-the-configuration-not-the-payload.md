---
status: FIXED 2026-09-08 — `records_pins` on `[[configuration]]`
found: 2026-09-08      # the emulator debt sweep, recording `rmt-chase` on the desk C6
area: lp-emu/lp-emu-validate/src/replay.rs (`pin_self_disagreements`, the `PinCapture::EveryFrame` arm)
class: gate-cannot-be-satisfied
related: [lp2025/2026-09-06-1001-esp-emulator/plan.md, docs/reports/2026-09-08-esp32c6-emulator-walk.md]
---
# A silicon capture of a pin payload can never replay, because only a machine has a pad we can read

**Symptom** — the first silicon capture of `rmt-chase`
(`lp-emu/transcripts/esp32c6/rmt-chase/silicon-esp32c6-2026-09-08-7d7ebfa62.txt`)
replays against `lp-emu:esp32c6:t1` with everything it can compare **equal**,
and fails anyway:

```text
  class             compared     equal    differ
  timing                  11         2         9
  pin                      3         3         0
  structural            3073      3073         0

  REPLAY FAILED (2 problem(s)):
    left transcript's payload `rmt-chase` claims a pin capture and has none
    pin capture: 0 decoded frames on the left, 768 on the right
```

3,073 structural comparisons equal — every one of the 768 frames' guest
checksums, frame for frame, against the emulator's — and the replay is still
red.

**Cause** — `replay()` reads `payload.pin_capture` and, when it is on,
requires **both** sides to carry a decoded pin log:

```rust
if payload.pin_capture.is_on() {
    for (t, side) in [(left, "left"), (right, "right")] {
        for problem in pin_self_disagreements(t, side)? { … }
    }
    match payload.pin_capture {
        PinCapture::EveryFrame if lp.len() != rp.len() => { … }
```

But a pin capture is not something a payload can bring with it. It is
something a **configuration** can produce: `lp-emu:*` decodes the pad off its
own signal fabric, and silicon cannot, because reading a real pad needs an
instrument nobody has put on this bench. The plan and the trust table both
already say so — `pins` grades `modeled` for every configuration with the
reason "the fabric and the decoder are in the machine, not on a scope … No
silicon pin transcript exists yet; a logic-analyser capture … would be the
`measured` step", and the walk record's §9 lists a silicon pin transcript as
not run.

So the runner asks silicon for the one thing the system has written down that
silicon cannot give.

**Why it matters** — it is not the red that is the problem; it is what a red
teaches. A committed transcript that can never replay is a booby trap: the
next person to record a pin payload on a board gets a failure whose message
("claims a pin capture and has none") reads like a bad recording, and the
useful half of the result — the 3,073 equal structural comparisons — is
below the failure where nobody looks.

**What would close it** — say it where everything else in this system is
said: **in the configuration**. `validate.toml`'s `[[configuration]]` gains a
stated capability, defaulting to false —

```toml
[[configuration]]
name = "lp-emu:esp32c6:t1"
records_pins = true    # the pad is decoded off the machine's own fabric
```

— and `replay()` requires a pin log only from a side whose configuration
declares one. Where the two sides disagree about it, the pin class is
reported as *not compared, and why*, the way timing is reported rather than
gated (PD9). Stated, never inferred, which is the rule this file's neighbours
already follow: a configuration that says nothing records no pins, because
silence is not a capability any more than it is trust.

Deliberately **not** inferred from the configuration's name prefix. `lp-emu:`
happens to be the only family with a modelled pad today, and a rule that
reads the name would be right by accident and wrong the first time a
configuration is a board with a logic analyser on it — which is exactly the
capture this defect is waiting for.

**Scope note** — the fix changes what every replay of a pin payload means, so
it wants its own change with the M5 replays re-run, not a rider on the PR that
found it. Until then
`lp-emu/lp-emu-validate/tests/m7_replays.rs::the_silicon_chase_agrees_frame_for_frame`
pins the half that is real — structural equality against the emulator — and
names these two problems as this defect rather than as a result.

## Closed, 2026-09-08

Done as the shape above describes, the same day it was filed.

`ConfigurationEntry` gains `records_pins`, `#[serde(default)]` false, and
`validate.toml` states it on `lp-emu:esp32c6:t1` and `:t2` and nowhere else.
`replay()` reads it out of the embedded table for each side — the file is
compiled in with `include_str!`, so this needed no plumbing and no sidecar
change, and no transcript had to be re-recorded.

Where a side records none, the pad is not compared and the report says so **in
the table**:

```text
  class             compared     equal    differ
  timing                  11         2         9
  pin                      3         3         0
  structural            3073      3073         0
  pin capture      not compared   — silicon:esp32c6 records none
```

The row is labelled `pin capture` rather than `pin` for a reason worth
keeping: the `pin` **class** was compared on the line above and agreed.
`ws281x-telemetry`'s trips, skips and errors are pin-class claims the guest
makes about its own driver and they arrive in the console. What is missing is
the decoded **pad** — a different reading of the same pin, and the only one an
instrument could confirm. Two rows saying "pin" with different answers would
have been worse than the failure this replaces.

The silicon chase now replays clean: 3,073 structural comparisons equal, the
pin class compared and equal, the pad not compared and named.

**Gates.** `m3`–`m6` replays re-run unchanged (7 / 4 / 9 / 12). `m7_replays`
is 9 tests, three of them this rule: the silicon chase reports the pad as not
compared and passes; two emulator grades that both record pins still compare
the pad as before, with no note; and `validate.toml` states the capability on
exactly the two configurations that have a modelled fabric. That last one is
what keeps the rule stated — a board with a logic analyser on it sets `true`
and needs no code change.
