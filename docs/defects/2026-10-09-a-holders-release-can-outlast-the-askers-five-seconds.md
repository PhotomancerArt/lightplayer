---
status: fixed
found: 2026-10-09      # how: e2e (`just walk-two-tabs-emu`, 2 of the 7 runs that reached the take-over; a desk with sibling builds running)
fixed: c751b84ca
area: lpa-studio-core `device_frame_feed.rs` (`feed_target`) × `board_hold/hold_answer.rs` (`RELEASE_PATIENCE_SECS`) × `take_over_state.rs` (`ASK_PATIENCE_SECS`)
class: state-conflation
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

**Root cause** — The card's frame feed read "the port is open" off the
roster's presence, and presence stays `Open` from the moment a Disconnect
folds until the port's close comes back to the fold. A holder lets a board
go inside one actor batch: the ask is heard, `settle_device_records` writes
the picture and folds the Disconnect (`run_due_hold_releases`), and the
same batch's tick then runs the card feed. When the holder's card was
showing the board's picture and its 150 ms gap had passed, `feed_target`
picked the device and the pull sent a request on the port being closed.
Nothing answers that request, so the pull waited out the shared-link reply
budget (`RESPONSE_BUDGET`, 5 s) with the actor stuck behind it. The port's
close had come back within milliseconds (the pump drained it), but its fold
and the release's own `Due` wake sat queued until the pull gave up, and
`Released` went out in the next batch, about 5.5 s after the ask.

A walk with the hold flow and the actor's batches traced (2026-10-10, page
B holding at step 4) showed it step by step: ask heard → picture written
and Disconnect folded at +99 ms (presence still `Open`) → the pump drained
`Closed` at +107 ms → that batch took 5,504 ms, 5,404 of them in the card
feed → `Released` at +5,504 ms. The second failing run looked the same
(feed 5,390 ms). Whether the feed came due inside the release's batch was a
race on its 150 ms gap, which is why it came and went.

A second weakness made the two timeouts fragile even without the stall: the
holder's 3 s close fallback started after the picture write, so nothing
bounded its whole release inside the asker's 5 s.

**Fix** — `feed_target` never targets a device whose connection intent is
Disconnected (the model has asked for its port closed), so no pull starts
on a port that is being let go. And the holder's release is now one budget
counted from the ask: `RELEASE_PATIENCE_SECS` (3 s; it was
`RELEASE_CLOSE_PATIENCE_SECS`, counted after the picture write). The lock
goes by then whether or not the port has closed, and a compile-time check
in `take_over_state.rs` holds that budget plus 2 s inside `ASK_PATIENCE_SECS`.
No "letting go…" note was needed: with nothing stalling the actor the answer
takes tens of milliseconds, and the holder's longest release (a close that
never comes back) is now 3 s from the ask. The walk's report notes any
answer over 1 s.

**Regression coverage** — `t7_a_feeding_holder_answers_within_a_second`
(`studio_device_e2e_tests/board_hold_tests.rs`: each turn in the actor's
order, a holder whose card is feeding, the answer within 1 s of the ask on
the injected clock; 5.01 s before the fix),
`t7_a_holder_whose_port_never_closes_answers_inside_the_askers_patience`,
`hold_answer::a_slow_picture_write_does_not_stretch_the_release`, and the
compile-time budget. The walk on a loaded desk (`--steps 3-4`): before the
fix, step 4 failed in 2 of 2 runs (5,504 and 5,480 ms); after it, 6 of
6 runs passed both take-overs, the 12 answers taking 69–341 ms (load
average 41–103), and no actor batch took longer than 700 ms.

**Lesson** — Two timeouts in series have to be written as a budget: the
asker's patience is the holder's whole release, not its first step, and the
holder's own fallback must fit inside it with room for the machine to be
slow. A passive lane that awaits inside the actor also turns any request
that can never be answered into a stall of everything queued behind it, so
it must ask whether the model has already let the resource go, not only
whether the evidence still reads open.
