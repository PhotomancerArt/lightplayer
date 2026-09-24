---
status: fixed
found: 2026-09-24      # ci — reddened main at e226fb28 (#793's merge)
fixed: this change
area: lp-emu/esp/lp-emu-esp32c6 tests/boot_no_radio.rs (`the_no_radio_image_runs_three_seconds_strict_to_the_idle_loop`)
class: unenforced-test-precondition
related: []
---
# boot_no_radio asserted that no tick handler was in flight at the 3 s deadline

**Symptom** — `Emulator C6 (x64)` failed with
`assertion failed: thresh_down >= handler_entries.len()` on run 35941350398
(`65f06de5e`) and on run 35946788718, the push of main's `e226fb28` (the #793
merge), and passed on `a696de28` and `ebf63d46`. Each result was deterministic
for its build and flipped between commits that touched nothing near the tick.

**Root cause** — the gate runs the no-radio image for exactly 3 s of emulated
time (480,000,000 cycles at t1) and then compared three raw counts: handler
entries (`mxint_clear = 0x00010000`), threshold raises (`mxint_thresh = 2`) and
restores (`mxint_thresh = 1`), requiring restores ≥ entries with no slack. The
deadline is not a quiet point. When a tick fires in the last ~1,000 cycles, its
handler is entered and raised but the run stops before the restore. On
`e226fb28`'s image (emulator at `ebf63d463`), the counts were entries 4,121,
raises 4,121, restores 4,120. The last entry was at cyc=479,998,995, 1,005
cycles before the deadline. The slowest completed handler took 1,140 cycles.
Whether a build puts a tick in that window depends on code layout, so the test
depended on something it never set up. Its `int_clr` bound already allowed one
tick of slack (`2 * entries - 2`); the threshold bounds did not.

**Fix** — the test now walks the trace in order and pairs each entry with its
raise and restore (`walk_handlers`). A second entry before the previous
handler restores panics, because that is a lost restore. Only the last handler
may be unfinished, and only if it was entered within twice the slowest
completed handler's span of the deadline. That second rule catches a handler
that hangs mid-run, which leaves no later entry to trip the pairing. The
`int_clr` count bound is unchanged. `schedule()`'s second clear is not always
inside the raise/restore span (6,241 of 8,243 clears fall inside on
`e226fb28`'s image), so it cannot be paired per handler.

**Regression coverage** — two ELF-free tests in the same file run on every
`cargo test`. `the_handler_walk_names_the_one_the_deadline_cut_off` checks the
edge case, and `the_handler_walk_refuses_a_restore_lost_mid_window` checks the
lost restore. The gate test, still `#[ignore]`d, passes on both
`e226fb28`'s no-radio image (one handler in flight at `Raised`) and
`ebf63d46`'s (none in flight). Main's old test fails on the first image and
passes on the second.

**Lesson** — a run that stops at a fixed emulated deadline stops mid-activity.
Any count of paired events (enter/leave, raise/restore, start/end) taken from
it must allow the one pair the deadline cut off, and nothing more. Pair the
events in order instead of comparing totals, so a real loss mid-run still
fails. Also check the other bounds in the same test against each other: here
the `int_clr` bound already had the slack and the threshold bounds beside it
did not.
