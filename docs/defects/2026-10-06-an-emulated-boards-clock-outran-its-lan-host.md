---
status: open
found: 2026-10-06      # how: e2e (PR B's emulated LAN uploads, lp-emu:esp32c6:t1+net=lan)
area: lp-emu/esp/lp-emu-esp-common seam/net (shared_lan.rs); the USB door
class: fidelity
related:
  - docs/defects/2026-10-06-a-lan-link-strands-the-heap-below-the-load-floor.md
  - docs/defects/2026-10-06-the-virtual-lans-forward-kept-a-moved-boards-old-connection.md
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

**Proposed fix (built and measured, NOT shipped in #993)** — the patch is kept
as `data/pr993-lan-host-pace.patch` in the plan directory
(`lp2025/2026-10-05-1903-wifi-link-c6`). It is held back because it changes an
emulator invariant (`lp-emu/esp/README.md` §Determinism: the host's clock
never sets the pace of an emulated run) and costs an idle hosted board ~⅓ of
its speed, which is a decision for the emulator's owner, not a fix to slip
into PR C; and because a variant that made up the sleep overshoot surfaced an
unexplained `[RECOVERY] io task silent > 2000 ms` (below). Nothing is lost
today — the host drops the board's resends as duplicates — so the cost of
leaving it open is wasted frames, not wrong behaviour. What the patch does: a
host on the LAN sets its pace (`lan_host_pace.rs`, `shared_lan.rs`):
while a host is connected through any forward, a self-driven or wall-clock LAN
bounds each board's next pump to one millisecond of its guest time
(`HOST_PACE_STEP_US`, so an idle skip cannot leap past what the host could have
said) and makes a board whose clock has run ahead of the host's wait at its
pump. A board behind the host is never hurried and owes no sprint; with no host
connected nothing waits, and a runner-driven LAN is untouched. The wait is
outside the LAN's lock, and a wasm build (which binds no forward) imports no
sleep.

Emulated (`lp-emu:esp32c6:t1+net=lan`, lp-emu at `f3feec073` plus the patch),
four uploads of `projects/test/basic` to one fresh board, then `lp-cli link rtt`:

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

**Coverage the patch carries** — `lan_host_pace::tests` (the pace's rule against host
time it is handed: ahead waits the difference, behind is not hurried, a long
skip waits at most the cap, a restart or a release starts again) and
`shared_lan::tests::a_connected_host_paces_the_lan_until_it_leaves` (a host
connected through a forward engages the pace and bounds the next pump; once it
has gone, neither). The resend counts themselves are not gated: the forward is
a host socket, and an emulated test never gates on the host's clock.

**Lesson** — an emulated board and a wall-clock peer are on two clocks, and an
idle board's clock runs many times faster than the peer's. Every timer that
measures a round trip to the peer (a resend timer, a probe, a keepalive's
stall) is then wrong by that factor, silently. The USB door has the same shape
and is not paced yet.

**Also seen while measuring, not understood** — pacing that made up each
sleep's overshoot (up to 10 ms of catch-up) ran the board at 1.00× but, in 3 of
3 runs, logged `[RECOVERY] io task silent > 2000 ms` and one ~3 s request; the
no-catch-up patch showed neither in 2 of 2. A latent stall under a different
traffic timing, on the firmware or the emulator side.

**Firmware/preset proposals** (PR B's to decide, sent to it): Nagle off on the
LAN endpoint's socket (`lan_endpoint_task.rs`, each frame is one whole
WebSocket message), `TCP_TX` ≥ 4 KB so a full lp-link window fits, and a LAN
`min_rto` of 200 ms or more (`lan_link_config.rs`, and `LinkConfig::ws()` on the
host) since TCP never loses a frame; `ack_every` 4 is unreachable with window 2.
