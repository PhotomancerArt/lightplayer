---
status: retired
since: 2026-07-31
logged: 2026-07-31
retired: 2026-08-01
area: lp-fw/fw-esp32c6/src/output/rmt_ws281x_driver.rs
related:
  - lp-fw/lp-ws281x/
  - 2026-07-31-0720-s3-led-output-4ch (plan dir)
  - 2026-08-01-1459-rmt-priority-hli (plan dir, P1+P2)
  - docs/adr/2026-07-31-lp-ws281x-multi-channel-driver-adoption.md
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

- **2026-08-01 (what made it unacceptable)** — Both named triggers arrived at
  once. M5 (`2026-08-01-1459-rmt-priority-hli`) needs `lp-ws281x`'s telemetry
  counters on the C6 to attribute its 29 % frame truncation under a WiFi scan,
  and Yona asked for the second channel the board has always had the hardware
  for. Priority work on the legacy driver would have been dead work.

- **2026-08-01 (retirement)** — P1 implemented `RmtHw` for the C6's RMT
  (`src/output/rmt/c6_rmt.rs`), P2 made it the only driver: legacy
  `rmt_ws281x_driver.rs` and `rmt/{buffer,channel,config,interrupt}.rs`
  deleted, `fw-esp32-common::output::rmt_state` deleted with its last
  consumer, and the manifest's second `/rmt/ws281x1` declared. Desk smoke on
  the 3-strip jig: both channels transmit concurrently (D10/GPIO18 → slot 0,
  D9/GPIO20 → slot 1), 1 158 frames each with `trips=0 skips=0 errors=0` and
  `refills == wanted`. The swap **saved** flash rather than spending it —
  2,873,632 B against the legacy image's 2,876,320 B (−2,688 B), because the
  legacy driver's own ISR/refill/pulse tables outweighed the shared core.
  Two consequences worth knowing: the harnesses' `LedChannel` API survives as
  a thin shim over the shared driver (`src/output/rmt/led_channel.rs`, harness
  builds only), and the driver-level duplicate of the 256-LED cap went with
  the legacy file — `Esp32OutputProvider` is now the single cap site on this
  chip.

**Exit criteria** — `fw-esp32c6/src/output/` depends on `lp-ws281x` the same
way `fw-esp32s3/src/output/rmt/` does, and `rmt_ws281x_driver.rs` /
`LedChannel` are deleted. **Met 2026-08-01**, with one deliberate deviation:
the *name* `LedChannel` survives as a harness-only shim over the shared driver
rather than being deleted outright, because five hardware harnesses drive a
strip without a registry. Nothing of the legacy implementation remains behind
it — no second ISR, no second refill loop, no `rmt_state`.
