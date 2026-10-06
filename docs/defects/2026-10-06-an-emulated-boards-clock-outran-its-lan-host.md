---
status: fixed
found: 2026-10-06      # how: e2e (PR B's emulated LAN uploads, lp-emu:esp32c6:t1+net=lan)
fixed: this change
area: lp-emu/esp/lp-emu-esp-common seam/net (shared_lan.rs); the USB door
class: fidelity
related:
  - docs/defects/2026-10-06-a-lan-link-strands-the-heap-below-the-load-floor.md
  - docs/defects/2026-10-06-the-virtual-lans-forward-kept-a-moved-boards-old-connection.md
  - docs/defects/2026-10-06-the-io-task-goes-silent-for-2-s-under-paced-lan-traffic.md
  - lp2025/2026-10-05-1903-wifi-link-c6 (PR C, #993)
---
# An emulated board's clock outran its LAN host, and its link resent frames TCP had already delivered

**Symptom** — every upload over an emulated board's LAN forward
(`lp-cli upload projects/test/basic lan:127.0.0.1:<port>` against
`lp-cli emu run --lan`) left the board's own counter line reading dozens of
resends: `[lan] link link1: frames in 128 out 221 · … · resends 95`, then 84,
74 and 83 on the next three uploads to the same board. A LAN link is one
WebSocket over TCP: nothing is ever lost, so lp-link should never resend.
The host's counters agreed nothing was lost (`host_link_counters`: 0 resends,
42 duplicates received — the board's resends arriving a second time).

**Root cause** — the emulator's, not lp-link's or the LAN preset's. The board's
timers count guest time; the host's count wall time; and nothing tied the two
together. An idle board's `wfi` skips straight to its next timer, so its clock
ran **7.6–7.9× the host's** (measured ping to ping through the forward, from
the board's seam trace). A WebSocket round trip of 1.2 ms of host time (p50;
p90 1.6 ms) was 9.6 ms of the board's (p90 12.1, max 17.6), and an lp-link
round trip — the host holds its ACK 5 ms (`ws()`'s `ack_delay`) — was tens of
milliseconds on the board's clock, past its tail probe (10 ms floor) and its
resend timer (20 ms floor). So the board resent frames the host had not yet
had the time to acknowledge. The same race showed on the USB door (a TCP
client over `serial:tcp://`): 54 resends per upload there, hidden mostly by the
C6 board's 200 ms USB resend floor.

It was **not** only wasted frames: in the emulated Wi-Fi walk (`just
walk-wifi-emu lan`, runs 6 and 9 at the shipping tree, unpaced) a board
running a project with Studio's LAN link open logged `radio link link1: a
reply still not out of the frame buffer after 5000 ms (2048 B held) —
closing` with `tick=5040ms` on every redial and reset itself (`rst:0x10
(LP_WDT_SYS)`, 3 and 12 times), because 5 s of its clock passed while the
host had been given well under 1 s to drain the reply. With the host pace
(runs 7, 8) none of that happened and the walk passed 10/10.

The pace was first built as a patch and held back from #993
(`data/pr993-lan-host-pace.patch` in `lp2025/2026-10-05-1903-wifi-link-c6`),
because it changed an emulator invariant (`lp-emu/esp/README.md`
§Determinism then said the host's clock never sets the pace of a run) and
costs an idle hosted board ~⅓ of its speed: the emulator owner's decision.

**Fix — an explicit pace, whose default is the host's** (Yona's decision,
2026-10-06: clock advance is controlled; 1× for patterns and anything a
person or wall-clock peer talks to, as fast as possible for unit tests, and
that is an option). A run has a **pace** (`lp_emu_esp_common::seam::net::Pace`,
`lan_pace.rs`), set by its host: `lp-cli emu run --pace realtime|max`, `emu
serve --pace …` for every board and `pace=realtime|max` per `--board`
(beside `lan=`, `seams=`).

- **Unset** (the flag left out — every existing run, CI, the lockstep runner,
  the tab, every test): the patch held back in #993, unchanged. While a host
  is connected through any of a self-driven or wall-clock LAN's port
  forwards, each board's next pump is bounded to 1 ms of its guest time
  (`HOST_PACE_STEP_US`, so an idle skip cannot leap past what the host could
  have said) and a board whose clock has run ahead of the host's waits at its
  pump (`HostPace`, `lan_host_pace.rs`; outside the LAN's lock, at most 50 ms
  a wait). A board behind the host is never hurried and owes no sprint, and a
  sleep's overshoot is not made up. With no host connected nothing waits.
- **`realtime`**: the same hold for the whole run, host or no host (1×).
  It is held at the board's LAN pump, so it needs the network seam engaged on
  a LAN the board drives itself: the C6 refuses it at build with no `net=lan`
  asked for (`--seams none`) or on a runner's LAN, and a chip start where the
  seam does not engage (an older image) ends the run (exit 64); a blank ROM-up
  chip runs nothing to pace until it is flashed and is let be until then.
- **`max`**: never paced, even with a host connected.

A set pace is in the configuration label after the seam atoms,
`lp-emu:esp32c6:t1+net=lan@pace=realtime` (`…@pace=max`); an unset one adds
nothing, so every existing label stands. `lp-emu-validate` reads the suffix
(never as a `+` seam atom) and refuses to `record` or `run` a paced
configuration: `realtime` is wall-clock dependent, and a transcript must be a
function of the image. The catch-up variant (below) is **not** shipped.

The USB door's clock has the same shape and is paced only when the board is
also on a LAN that holds it (`realtime`, or a LAN host attached): a
`--seams none` run has no pace to set.

Measured with the held-back patch, which is the shipped unset pace
(`lp-emu:esp32c6:t1+net=lan`, lp-emu at `f3feec073` plus the patch), four
uploads of `projects/test/basic` to one fresh board, then `lp-cli link rtt`:

| | before | after |
|---|---:|---:|
| board's resends per upload | 95, 84, 74, 83 | 9, 14, 12, 13 |
| board's frames out per upload | 221, 218, 207, 217 | 135, 147, 145, 147 |
| board clock ÷ host clock, idle, connected | 7.9× | 0.66× |
| one WebSocket round trip on the board's clock, p50 | 9.6 ms | 1.1 ms |
| board's resends over a whole `link rtt` session | 42 | 2 |

The residue (9–14 per upload) is not the emulator's clock: with the gateway's
TCP acknowledging at once instead of after smoltcp's 10 ms delayed ACK, the
same four uploads resent **0, 0, 0, 0**. It is the board's TCP (Nagle on, a
2 KB send buffer under its two 1 KB lp-link frames) waiting on its peer's
delayed ACK, against lp-link's 10 ms probe and 20 ms resend floors on a LAN
link — which a real host's delayed ACK (40 ms and up) would make worse, not
better. That is the firmware's and the LAN preset's to decide, not the
emulator's: the gateway keeps modelling a host that delays its ACKs.

**Coverage** — `lan_host_pace::tests` (the pace's rule against host time it
is handed: ahead waits the difference, behind is not hurried, a long skip
waits at most the cap, a restart or a release starts again);
`shared_lan::tests`: `a_connected_host_paces_the_lan_until_it_leaves` (unset:
a host connected through a forward engages the pace and bounds the next pump;
once it has gone, neither), `a_max_pace_never_waits_even_with_a_host_connected`,
`a_realtime_pace_holds_the_board_with_no_host`,
`a_runners_lan_refuses_a_realtime_pace`; `lp-emu-esp32c6`'s
`tests/seam_net_pace.rs` (the pace reaches the LAN and the label, an unset one
neither; `realtime` refused with no network seam, on a runner's LAN, and at a
chip start the seam does not engage); `lp-emu-validate`'s
`a_pace_follows_the_atoms_and_is_never_a_seam` and
`a_paced_run_never_records_or_runs`; `lp-cli`'s argument tests for both hosts
and the board option, and the label's parity across the fence. The resend
counts themselves are not gated: the forward is a host socket, and an
emulated test never gates on the host's clock.

**Lesson** — an emulated board and a wall-clock peer are on two clocks, and an
idle board's clock runs many times faster than the peer's. Every timer that
measures a round trip to the peer (a resend timer, a probe, a keepalive's
stall) is then wrong by that factor, silently. The USB door has the same shape
and is paced only through the LAN.

**Also seen while measuring, not understood** — pacing that made up each
sleep's overshoot (up to 10 ms of catch-up) ran the board at 1.00× but, in 3 of
3 runs, logged `[RECOVERY] io task silent > 2000 ms` and one ~3 s request; the
no-catch-up patch showed neither in 2 of 2. A latent stall under a different
traffic timing, on the firmware or the emulator side, filed open as
`2026-10-06-the-io-task-goes-silent-for-2-s-under-paced-lan-traffic.md`; the
shipped pace is the no-catch-up one, so `realtime` runs at about two thirds of
wall speed when idle rather than exactly 1×.

**Firmware/preset proposals** (PR B's to decide, sent to it): Nagle off on the
LAN endpoint's socket (`lan_endpoint_task.rs`, each frame is one whole
WebSocket message), `TCP_TX` ≥ 4 KB so a full lp-link window fits, and a LAN
`min_rto` of 200 ms or more (`lan_link_config.rs`, and `LinkConfig::ws()` on the
host) since TCP never loses a frame; `ack_every` 4 is unreachable with window 2.
