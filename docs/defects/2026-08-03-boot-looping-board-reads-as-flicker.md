---
status: open
found: 2026-08-03      # how: gate-1 capture sitting (s5, WLED image)
area: lpa-studio-core device session + auto-connect sweep
class: unrecognized-recurring-state
related:
  - docs/adr/2026-08-03-studio-runs-n-device-sessions.md
  - lp-app/lpa-link/testdata/device-traces/s5-foreign-firmware.failed.jsonl
---
# A board stuck rebooting reads as flicker, not as a state

**Symptom** — a board that resets repeatedly (here: a WLED image whose
flash mode did not match the board — `SHA-256 comparison failed`, then
`rst:0x7 (TG0WDT_SYS_RST)`, looping) presents to Studio as a card that
appears and vanishes. Yona, gate-1 sitting: *"the device just appears
for a moment then disappears."* The captured trace is the whole pattern:

```
state · → booting ; pool install (Device) ; state booting → gone
flow  connected → discovering-endpoints → … → connecting
state · → booting ; pool install (Device) ; state booting → gone
```

**Why it is a defect, not just honest reporting.** Every individual
transition is correct — the device really did go away each time. But
nothing in the product names the PATTERN, and the auto-connect sweep
re-attaching each cycle actively hides it: the user sees flicker and no
explanation. A board with a bad flash is not exotic (wrong image,
interrupted write, brownout, insufficient USB power); "it blinks in and
out of the list" is a terrible answer for it.

**What the device can be told, and what it cannot.** The reset REASON
(`rst:0x7 TG0WDT_SYS_RST`, `SHA-256 comparison failed`) is in the ROM
boot output — which the classifier already reads for chip detection, so
the evidence is on the wire. Even without parsing those lines, the shape
is available: repeated short-lived sessions on ONE endpoint inside a
short window. The M0 device event log already records every
install/`gone` with a timestamp and endpoint id, so the detection input
exists today.

**Not yet done** — no fix attempted. Candidate direction: a card state
for a board that keeps restarting (N sessions ending `gone` within T
seconds on one endpoint), which the sweep should also back off from
rather than re-attaching into a loop; the ROM reset reason as the
sub-line when it is available. Sequencing note: M5 revisits the sweep
and is the natural home for the back-off half.

**Endpoint identity is NOT stable across the loop** (found 2026-08-03
while planning the fix). The two captured cycles ran on DIFFERENT endpoint
ids — `browser-serial-esp32-port-1`, then `…-port-2` — for one physical
board on one port. `create_granted_endpoint` mints its id from a monotonic
counter and only reuses one when the JS layer reports the same `port.id`,
which is keyed by SerialPort object identity; a USB re-enumeration
produces a new object. So the obvious fix shape — a per-ENDPOINT sweep
back-off — would never fire here. (Checked that this is not the retired
L1 dedupe bug: the capture postdates that fix, and the JS session map is
never pruned.) The same question decides whether a card-owned op flow can
follow its board across the replug that ends a recovery write; both are
carried in `lp2025/2026-08-03-1710-unstable-board-handling`.

**Regression coverage** — `s5-foreign-firmware.failed.jsonl` is a real
capture of the loop and can serve as the fixture; the trace replay
harness (`lp-app/lpa-link/tests/trace_replay.rs`) already parses it.

**Lesson** — honest per-event reporting is not the same as an honest
picture. Each transition here was individually true and the composition
still misled, because the state that mattered lived in the RATE, not in
any one event.
