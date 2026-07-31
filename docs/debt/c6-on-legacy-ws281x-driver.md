---
status: carried
since: 2026-07-31
logged: 2026-07-31
area: lp-fw/fw-esp32c6/src/output/rmt_ws281x_driver.rs
related:
  - lp-fw/lp-ws281x/
  - 2026-07-31-0720-s3-led-output-4ch (plan dir)
---
# The C6 still runs its own legacy single-channel WS281x driver, not `lp-ws281x`

**Shape** — `lp-fw/lp-ws281x` is now the multi-channel, hardware-validated
WS281x driver core (ping-pong refill, bit cursor, guard word, `RmtHw` seam),
adopted by `fw-esp32s3` for 4 concurrent RMT channels. `fw-esp32c6` was
deliberately left untouched: it still runs its own single-channel
`rmt_ws281x_driver.rs` / `LedChannel` implementation — the driver `lp-ws281x`
descends from and was written to replace. The two drivers now diverge:
`lp-ws281x` has the bit-cursor refill (works for any half-size, not just a
whole number of LEDs), per-channel configurable timing, and no start-of-frame
guard race; the C6's ancestor has none of that and is limited to one channel
even though the C6 hardware exposes two RMT-capable outputs.

**Why it is acceptable now** — the swap was explicitly out of scope for the
4-channel S3 output plan (`2026-07-31-0720-s3-led-output-4ch`, "Out" section:
"swapping the C6 onto lp-ws281x (follow-up plan — the C6 driver and
`fw-esp32-common::output::rmt_state` stay untouched)"). The C6's flash budget
is already tight (partition ~99.7% at last measurement,
`docs/defects/2026-07-28-esp32c6-app-partition-overflow.md`) and its own
driver is stable in production use, so absorbing a driver swap alongside a
different chip's bring-up risked destabilizing the reference board for no
immediate gain.

**What makes it unacceptable later** — a second channel is wanted on the C6
(the board exposes the RMT hardware for it and the plan's acceptance criteria
already note "C6 has 2"); the C6's driver picks up a bug `lp-ws281x` has
already fixed (e.g. the start-of-frame guard race); or maintaining two
independently-evolving WS281x drivers becomes its own tax (bugs fixed in one
not the other, duplicated `MAX_LEDS` — see
`docs/debt/output-channel-led-cap-silent-truncation.md`).

**The fix** — port `fw-esp32c6` onto `lp-ws281x` the way `fw-esp32s3` already
is: implement `RmtHw` for the C6's RMT peripheral (`Ws281xDriver`'s host-side
sequencing already exists and is chip-agnostic), retire
`rmt_ws281x_driver.rs`, and size-check the swap against the C6's tight
partition before committing to it — `lp-ws281x`'s code density on the C6 is
unmeasured.

**Workarounds** — none needed; the C6's own driver works for its current
single-channel deployment. Anyone extending the C6 to a second channel should
port to `lp-ws281x` first rather than duplicating the S3 driver's channel
plumbing into the legacy code.

**Incident log**
- **2026-07-31** — Filed at the close of the S3 4-channel output plan, per
  that plan's explicit scope note; no incident, a deliberate deferral being
  recorded as a condition rather than left implicit.

**Exit criteria** — `fw-esp32c6/src/output/` depends on `lp-ws281x` the same
way `fw-esp32s3/src/output/rmt/` does, and `rmt_ws281x_driver.rs` /
`LedChannel` are deleted.
