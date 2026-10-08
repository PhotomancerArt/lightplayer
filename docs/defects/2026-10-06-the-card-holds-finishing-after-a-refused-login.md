---
status: open
found: 2026-10-06      # how: hardware walk (the M7 agent pre-walk, Studio in Mac Chrome over Bluetooth, fixture C6)
area: lpa-studio-core the update standing / words (`device_update_standing.rs`, `device_update_words.rs`) × the update host's remembered miss (E13)
class: untested-path
related:
  - docs/defects/2026-10-06-a-typed-password-unlock-cannot-log-in-to-core-only.md
  - lp2025/2026-10-05-0820-ota-studio-ble-updates (P4, P5)
---
# The card holds "Finishing the update… 0%" after the update ended `LoginRefused`

**Symptom** — when the core-side login was refused (see the related
defect), the update ended at once, but the card's firmware line stayed
"Finishing the update… 0%" for as long as the page was open (8.5 min
observed), with no offer and nothing saying a password was needed. A
fresh page connecting to the same core-only board did the same: the
no-click finish started, ended `LoginRefused` in 0.2 s, and the card said
"Finishing the update…" again.

**Mechanism (as read, not yet confirmed by a test)** — the standing is
computed from the board's facts (a transfer of this Studio's build pending)
and reads as E5 ("finishing"); the run's end is remembered as a miss for
the no-click start (E13: no loop), but nothing turns that miss into the
card's words. The roadmap's words for this case are "needs the author
password" (E12's sentence, or the plan's future-work prompt).

**Not fixed here** — it needs a standing for "this run stopped at a
login" and a decision about the offer it carries; the PR's credential fix
removes the common way to reach it (a remembered typed password). Left for
the walk's triage.
