---
status: fixed
found: 2026-10-09       # walk: the boards-and-projects director's overnight walk run
fixed: this change
area: scripts/emu/walk-wifi-emu.mjs (the `usb` and `ble` lanes)
class: assumed-context
related:
  - docs/adr/2026-10-05-emulator-seams.md   # §11, the network seam, on by default
  - docs/reports/2026-10-06-wifi-emulator-walk.md
---
# The Wi‑Fi settings walk still expected firmware without Wi‑Fi

**Symptom** — `just walk-wifi-emu usb` and `just walk-wifi-emu ble` failed
at their second step, every run, on the page branch `claude/one-home-page`
(`2bb7e11b7`, the overnight walk run) and on `main` (`587bb7169`,
reproduced 2026-10-09):

```
— open: open the Wi‑Fi row: nothing saved, so it opens on adding a network by name
  ✗ waiting for the connect page (Not set up., type the name): page evaluation failed: Error: wait deadline
```

The panel on the screenshot said "Not connected. Pick the board's network:",
with an empty Nearby list and "Other network…". The lanes `lan`,
`studio-lan`, `studio-lan-reset` and `studio-relay` passed.

**Root cause** — the two lanes were written at Wi‑Fi M5 (`bf334b8fd`,
2026-10-05), when the C6 image could save a network but had no station.
Their wait conditions were Studio's words for that firmware — "Not set up.",
`CANNOT_LIST` ("This firmware can't list networks. Type the name."), and the
in-row test's "this firmware can't connect to Wi‑Fi yet" — so the walk
carried a fact about the firmware (station `unsupported`) as a fixed
expectation instead of reading it from the board. On 2026-10-06 the
emulator's network seam (`net=lan`) became a soft default (ADR
`2026-10-05-emulator-seams.md` §11): the packaged `fw-esp32c6` on an
emulated board now has a station, scans (an empty LAN of its own when the
door names none) and joins. Studio drew the connect page for a board that
can scan, and the walk waited out its deadline for a page that no longer
exists. The walk is not CI, and every Wi‑Fi lane added since lives in its
own script, so nothing ran these two until the overnight run.

**Fix** — the lanes walk a board that has Wi‑Fi. The board sits on a
virtual LAN of its own (`lan=home`, a fixture the walk writes: two made-up
access points). The walk picks `lp-walk-net` from the board's scan, types
its password and presses Connect (the board joins; the row shows its own
lease), adds `lp-back-office` by name through "Other network…" (no access
point has it: Not in range, and the board stays joined), checks the chip
file, reloads (both read back, still joined), turns the cloud relay off and
forgets `lp-back-office`. Every claim keys off the board: its status over
its LAN forward (`lp-cli wifi status lan:<fwd>`, a path the page never
touches), the networks its scan heard, and the chip file. The password
check stays, for both passwords. A connect page that says "can't list
networks" now fails by name ("the board's firmware cannot connect to
Wi‑Fi … this lane needs a firmware with a station") rather than as a wait
deadline.

**Regression coverage** — none automated: the walk is not CI (headless
Chrome over the release bundle). Both lanes ran green on this change, twice
each, 9 of 9 steps, about 50 s a lane, on `lp-emu:esp32c6:t1+net=lan` (lp-emu
`d6c445a05`): the board joined `lp-walk-net` at `192.168.4.100`, reported
`lp-back-office` `last: notFound` while staying joined, read both back after
a reload, turned the relay off and kept `lp-walk-net` after the Forget. No
password on the page or in its console. The `lan`, `studio-lan`,
`studio-lan-reset` and `studio-relay` lanes still dispatch (`--dry-run`).

**Lesson** — a walk whose wait conditions are a capability's *absence*
breaks the day the capability ships, and a walk outside CI only finds out
on its next run, as a deadline that says nothing about why. When a
capability lands — here a seam turned on by default — grep the walks for
the words of its absence (`CANNOT_LIST`, "can't connect yet") in the same
change. And where a walk depends on what the firmware can do, make that a
named precondition, so the failure says "this firmware cannot connect", not
"the page was slow".
