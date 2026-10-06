---
status: fixed
found: 2026-10-06      # how: hardware-walk (PR #880's silicon re-check, bench XIAO C6 A0:F2:62:87:B4:8C, Mac Chrome over CDP)
fixed: this change
area: lpa-studio-core app/access/access_session.rs (AccessSession::logged_in, `auto_spent`)
class: state-conflation
related:
  - docs/defects/2026-10-06-a-bluetooth-reconnect-reads-the-old-links-loss.md (the other half of the desk check's finding 1)
  - lp2025/2026-09-28-1445-ble-on-lp-link (data/desk-check-2026-10-05/README.md finding 1; data/desk-check-2026-10-06/walk-2, walk-3)
---
# A typed unlock is not tried again after a drop

**Symptom** — PR #880's silicon re-check, after the lpa-link fix for the old
link's loss. A locked C6, a fresh Chrome profile: the board offered nothing
this browser held, so the Unlock sheet rose and the lab password was typed
with "Remember on this Mac" ticked ("Unlocked by desk-check-880b"). The
editor opened, Play, a knob. Then a power cut: the page held behind
"Reconnecting…", the board restarted, Web Bluetooth reconnected, and
the board closed the new link at its 10 s login deadline, twice ("no login
within 10 s — closing"). After 45 s the editor's hold ran out to
`/devices`, with the sheet up again (walk-2). The same walk on the same page
after a reload, where the remembered password was the FIRST thing tried,
rode out two power cuts and a reboot, each resumed in 7–9 s and logged in on
the new link (walk-3). The 2026-10-05 desk check had the same history
(typed first, then a power cut and a reboot) behind its finding 1, though
there the lpa-link defect hid the board from the model before this could
show.

**Root cause** — `auto_spent` meant two things. It is set when an
automatic try does not unlock, so that silent reconnects do not burn the
board's backoff on a password it refused. It was cleared by nothing but a
new page, so it also meant "nothing automatic may ever be tried on this
device again". On a board this browser holds no key for, the first
automatic try is the held keys, which match nothing (`NothingMatched`, no
wrong answer on the board's count). That spent it. The typed password then
unlocked the board and was remembered, but `auto_spent` stayed set. On the
next window `checked()` saw Locked + spent and raised the sheet, and
`next_step` only began a challenge for it. The remembered password that had
just worked was never sent, and the board dropped each link at its deadline.

**Fix** — a grant re-arms the automatic tries (`AccessSession::logged_in`,
`Granted`), whatever unlocked the board. What unlocked it once (a held key,
or a typed password now remembered) is what the next window reaches for.
A refused password still spends them, as before.

**Regression coverage** —
`a_password_typed_once_unlocks_the_restarted_board_by_itself`
(`lpa-studio-core`, `studio_device_e2e_tests/ble_drop_tests.rs`: a locked
board over an untrusted fake link, the sheet answered with a remembered
password, a restart under the editor, the resume with no sheet and an
answered pull). It failed before the fix with the card at "Needs a device
password" and the editor closed. Unit:
`a_reconnect_after_a_typed_unlock_tries_the_remembered_password`
(`access_session.rs`). Silicon: walk-3 of 2026-10-06 (remembered-first) and
the re-run after the fix (see the plan's desk-check record).

**Lesson** — a "don't retry" latch needs an explicit re-arm on success. A
latch that only a page load clears turns one empty try into a permanent
refusal to use what the user just gave us.
