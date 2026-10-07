---
status: fixed
found: 2026-10-07      # how: desk run (fixture C6, Mac Chrome via CDP, OTA M7 BLE speed pass, run s1)
fixed: 81077deee, 6f3fc621b
area: lpa-update `UpdateDriver` (a running engine's `N`/`A`) × the engine's channel 3 taking the tier its server's login granted the link; fw-esp32c6 `update_edge.rs` (`[OTA] refused`)
class: assumed-context
related:
  - docs/defects/2026-10-07-a-usb-host-on-the-board-closes-its-bluetooth-update-link.md (the drop that led here)
  - docs/defects/2026-09-25-a-knob-jump-over-bluetooth-kills-the-c6-ble-host.md (the same `-Zfmt-debug=none` blank, in a trouble-host line)
  - lp2025/2026-10-05-0820-ota-studio-ble-updates (ble-backup-fix.md, question 3)
---
# A reconnected Bluetooth link refuses the backup before its login lands, and says nothing about why

**Symptom** — after a Bluetooth link dropped mid-backup (run s1), Studio
reconnected in 2.0 s and the board answered four channel-3 messages with

```
[OTA] refused 
[OTA] refused 
[OTA] refused 
[OTA] refused 
```

(the reason empty), and 0.2 s after the reconnect Studio's update ended
`NeedsEngineLogin` with the card left at "Backing up current firmware… 0%".
The backup never resumed. Desk run c2b (fixed build of the first half) met
the same refusal on a link that dropped 21 s after the press, before the
board had answered a single `G`.

**Root cause** — two things.

1. A running engine's channel 3 takes the tier its server's login
   (channel 1) granted the link (`LinkMuxTransport` rule 5). A reconnect is
   a new link that holds nothing until Studio's access controller logs in
   on it — while the update driver sends `Q` the moment the leg starts and
   its four (now seventeen) `G`s the moment the board's `M` answers. The
   `G`s won the race and were refused `N`/`A`, and the driver read any
   running engine's `N`/`A` as final (`NeedsEngineLogin`), which is right
   only where no login was ever coming (lp-cli's U8: an untrusted USB link
   nobody logs in on). The driver assumed the new link carried the old
   one's grant.
2. The board's line was `log::warn!("[OTA] refused {r:?}")`, and the C6
   builds with `-Z fmt-debug=none` (`.cargo/config.toml`, −93 KB), which
   formats every `{:?}` as nothing. The radio mux's `session reset
   ({reason:?})` line was blank the same way.

**Fix** — `81077deee`: a driver waits out a running engine's `N`/`A` on
any link but its first (and once the board has answered a read-back): it
keeps the backup, asks `Q` again every second
(`ENGINE_LOGIN_RETRY_MS`), and stops `NeedsEngineLogin` only after 30 s
(`ENGINE_LOGIN_WAIT_MS`, counted from the first refusal and reset only by
progress, so a link that keeps dropping cannot keep it alive); the `M` that
answers a later `Q` resumes the backup from its first missing piece.
`6f3fc621b` extended it from "the board answered" to "any later link"
(c2b). `Refusal` gained `Display`, in words, letter first, and the board
logs `[OTA] refused on link 3: A: log in first`; the session-reset line
says its reason in words. No protocol change: `N`/`A` is v1's.

Desk evidence on the fixed build: run d2's reconnect met four refusals
(`[OTA] refused on link 3: A: log in first` ×4 in the board's log), and the
backup went on from 35 % on the next `Q` (then sat at 36 % on an answer lost
with the link up — the re-ask in the other entry). Run c3 cut the link at
the central at 40 % of a backup: it reconnected in 2.0 s and the backup went
on to 99 % with no click.

**Regression coverage** — `lpa-update` `tests/backup_refused.rs`
(`a_refused_read_back_stops_the_driver_with_needs_engine_login` — the first
link still stops at once;
`a_backup_cut_by_a_drop_waits_for_the_engine_login_on_the_new_link_and_resumes`;
`a_drop_before_the_first_answer_still_waits_for_the_login_on_the_next_link`;
`a_wait_for_the_engine_login_gives_up_and_stops_needs_engine_login`), the
host × board simulation (an idle link now ticks the driver before it
drops), and `lpc-update` `refusal.rs`
(`every_refusal_says_its_reason_in_words_letter_first`). `?ble=emu` cannot
show the race (the emulated board answers at the edit tier).

**Lesson** — "refused, log in" from a server whose login lives on another
channel is a statement about *now*, on *this* link: a client that reads it
as a verdict must know whether a login is on its way. And in an image built
with `-Z fmt-debug=none`, a `{:?}` in a log line is an empty string: every
line meant to name a reason needs `Display` or a literal. Still blank on the
C6 today, outside this change: trouble-host's `encountered error processing
ACL data for : ` (seen in d1's core-only log), `radio links: mode already
decided — {mode:?}`, `dropping frame … refused ({error:?})`, and the access
line `the device is open to {open:?} now`.
