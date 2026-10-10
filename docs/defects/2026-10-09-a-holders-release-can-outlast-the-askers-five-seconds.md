---
status: open
found: 2026-10-09      # how: e2e (`just walk-two-tabs-emu`, 2 of the 7 runs that reached the take-over; a desk with sibling builds running)
area: lpa-studio-core `board_hold/hold_answer.rs` (`RELEASE_CLOSE_PATIENCE_SECS`) × `take_over_state.rs` (`ASK_PATIENCE_SECS`)
class: unclassified
related:
  - docs/adr/2026-10-08-the-board-card-and-one-home-page.md (the amendment of 2026-10-09: take-over)
  - lp2025/2026-10-08-2330-one-tab-holds-a-board (p3-usb-holder-side.md, p4-usb-asker-side.md)
---
# A holder's release can outlast the asker's five seconds

**Symptom** — Twice in seven walk runs a take-over said "That tab didn't
answer" although the holder had let go: the asker's bar was striped, its
primary was Connect again, and the other tab's card said "Taken by another
tab". Once the asker was B (step 3), once A (step 4). The runs that passed
took 75 and 93 ms from the ask to the `Released` note (the last run's two take-overs).

The failing run's page logs (step 4): B heard the ask at +0 ms, wrote its
last picture and closed the port at +226 ms, and said `released` at
+5,649 ms. The asker's patience is 5 s, so A had already failed the ask
when the answer came, and an answer to an ask that is no longer waiting is
ignored. A second press (Retry) would have found the claim gone and opened
the board at once.

**Root cause** — Not established. What is known: the holder's release waits
for its port to close in the model (`PendingRelease::ready_to_release`, with
`RELEASE_CLOSE_PATIENCE_SECS` 3 s as the fallback), the port's own close had
completed 1 ms after it began, and the answer still came about 5.4 s later,
close to the fallback plus a late wake (3 s plus about 2 s). So either the
model did not see the close until the `Due` wake, or the wake itself ran
late. The pages' frame loops were healthy in the runs where this was
measured (about 62 frames a second), and a desk with other builds running is
where it happened. The two patience figures (3 s to close and 5 s to
answer) also leave only 2 s for the picture write, the disconnect and any
stall, so any hiccup in the holder reads as "didn't answer".

**Fix** — none yet. Candidates: have the asker's wait run from the moment
the holder says it is releasing (a `Releasing` note, with the asker's
patience restarting), or make the asker take a late `Released` for an ask
it already failed as an answer (the board is free; `Gone` already opens it
for a waiting ask, `take_over_freed`, so an ask that failed in the last few
seconds could be treated the same).

**Regression coverage** — none. The walk records each ask's time to answer
(`report.answerMs`), so a slow release shows in the report.

**Lesson** — Two timeouts in series have to be written as a budget: the
asker's patience is the holder's whole release, not its first step, and the
holder's own fallback must fit inside it with room for the machine to be
slow.
