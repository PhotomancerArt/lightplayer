# The cloud relay walk, run on an emulator: one board, an uplink, a relay

**Date** 2026-10-07 · **Plan** `lp2025/2026-10-06-0815-wifi-relay` (P9,
PR B) · **Command** `just walk-wifi-emu relay` · **Script**
`scripts/emu/walk-wifi-emu-relay.mjs` · **CI cell**
`lp-cli/tests/emu_relay_link.rs` (in `test-emu-serve`) · **Silicon twin**
the plan's P10 desk sitting (not run here)

> **STATUS: PASSED, 7/7.** The figures below are from the run at
> `a039064b3`; the label is `lp-emu:esp32c6:t1+net=lan@a039064b3` (the
> shipped `fw-esp32c6` image built from that commit, lp-emu at the same
> commit). Three earlier runs passed at `73f16f465`, `78739f4fb` and
> `4df5d1b3f`, with the same heap figures to the byte; those three carried
> a board log line this PR then took out again (section 2).
>
> **The one gate that failed is a heap gate, not a step.** With
> `projects/test/basic` loaded and a network session open, the largest
> free block is **13,448 B** (relay session) / **13,384 B** (LAN session),
> under the 16,384 B read gate (plan A3, R1). The relay's buffers cause
> it (section 3). Reported to the director, not tuned.
>
> **Round 2 (2026-10-07, section 6) fixed it** and corrected the gate it was
> read against. Sections 1–5 are the record of round 1 and are left as they
> were.

## 1. The setup

- One emulated C6 under `lp-cli emu run --lan <fixture> --flash <file>`.
  The fixture names an access point and one uplink:

  ```toml
  [[uplink]]
  name = "lightplayer.app"
  to = "127.0.0.1:<the relay's port>"
  ```

  The virtual LAN's gateway answers DNS for that name (and only that
  name) with `192.0.2.1`, hands itself out as the DNS server in DHCP
  option 6, and carries a board's TCP connection to `192.0.2.1:80` to the
  host address, the way its port forward carries one inward.
- The relay is an in-process `lp-cloud-server` (mem store) with two dev
  accounts, Alice and Bob (`lp-cli/tests/support/relay_cloud.rs`).
- The firmware is the shipped image: it dials `lightplayer.app:80` as a
  real board does. Nothing about the uplink is in the firmware.
- Two boots over one flash file, as a fielded board: the first run saves
  the network and installs Alice's key over USB, then ends at the board's
  heartbeat line `[relay] state=connected`; the second boots with both
  saved.

## 2. The steps, and the board's words for each

Each step is checked twice: by the cell's own assertions, and by the walk
reading the board's own words in the log — its console, or its status
answer over USB (`NetworkStatus.relay`, printed by the cell as `the board
answers (…): relay …` for each state it waited for).

| Step | What | The board's words |
|---|---|---|
| R1 | joined, no account key: no board at the hub | answers `relay noAccount` |
| R2 | the key installed (`AccessAdd`, as Studio does); after a reboot it registers by itself; the hub lists it with its LAN address (`…:80`) | `[relay] leg open to lightplayer.app`; answers `relay connected` |
| R3 | `relay:<mac>@<origin>` at Alice's tier (`edit`); `lp-cli upload projects/test/basic relay:…` passes the board's gates | `[relay] route N: link linkN, secure session opening (1024 B frames)` |
| R4 | Alice's key over `lan:` takes the one network session; her relay session is gone | `[relay] route N: closed (taken over by the same key)` |
| R5 | Bob through the relay while the LAN session holds: busy | `[relay] route N: the network link is in use — busy` |
| —  | a deploy (the relay away and back on its port): the board returns by itself | `[relay] leg open to lightplayer.app` again |
| R6 | Cloud relay off over USB: the board leaves the hub; on: it returns | answers `relay off`, then `relay connected` |
| R7 | Alice's key reset on the server, then a deploy: refused | answers `relay refused: unknownAccount` |

**Why status answers, not console lines, for R1, R2, R6 and R7.** The
board's console says its relay state only in the heartbeat (`[relay]
state=… routes=… rx=… tx=…`, every five emulated seconds), and the cell's
last steps end before the next one prints. A `[relay] now <state>` line on
every change was added (`78739f4fb`) and taken out (`a039064b3`): it cost
**688 B of core flash, which pushed the split image's core 224 B over a
32 KB page** and took the gated headroom from **123,028 B to 90,660 B**
(`just fw-esp32c6-size-check`, both sides measured). The core now sits
464 B under that page boundary: the next +464 B of core anywhere costs
32 KB of headroom at once.

## 3. Numbers

All on `lp-emu:esp32c6:t1+net=lan@a039064b3`. Times are **wall time on
this host** (an M2 Max, other worktrees building: load average 68 / 71 /
63 just after the run) and never a gate (AGENTS.md); the heap figures are
the board's own heartbeat and transfer.

**Heap** (free / largest free block, from the board's heartbeat):

| State | Free | Largest block |
|---|---:|---:|
| joined, no account key (no relay buffers yet) | 173,504 B | 94,324 B |
| booted with network and key saved, relay registered, no session | 166,512 B | 87,332 B |
| `projects/test/basic` loaded, relay registered, **a relay session open** | 56,784 B | **13,448 B** |
| `projects/test/basic` loaded, relay registered, **a LAN session open** | 56,716 B | **13,384 B** |
| probe: the same, Cloud relay on but **no account key** (buffers made, never dials), a LAN session open | 57,016 B | 13,448 B |
| probe: the same, **Cloud relay off at boot** (no relay buffers), a LAN session open | 63,940 B | 19,556 B |

The two probe rows are an uncommitted probe test on this branch (same
image and fixture, the cell's steps cut short), taken before the walk.

- **Registration costs ≈7.0 KB** of free heap (173,504 → 166,512), under
  the plan's 8 KB (A3).
- **The gate case fails.** The relay's buffers (6,921 B: TCP rx 2,048,
  tx 2,560, the WebSocket's receive buffer and one outgoing route frame)
  are allocated at boot whenever the board will join with Cloud relay on,
  account key or not (the plan's rule, so they sit low in the heap). With
  a project loaded that takes the largest block from 19,556 B (relay off)
  to ≈13,400 B, under the 16,384 B read gate.
  - Tried: TCP rx 1,024 B. Free +1,024 B, largest block 13,440 B: no
    help (the block is cut by where the buffers sit, not how big they
    are). Reverted.
  - Not tried: sharing the TCP buffers with the LAN endpoint (both sockets
    are live at once: the LAN listens while the relay holds its leg).
    Sharing the outgoing frame buffer (~1 KB) is possible.
  - This is the director's call (A3 says stop and report).

**Time**:

- **Boot → registered:** by the first heartbeat, at **5,000 ms of
  emulated uptime** (the join included); the heartbeat's cadence is the
  resolution. 9.1 s wall (6.8–11.5 s across the runs).
- **Requests through the relay**, 20 status requests one after another
  with `projects/test/basic` rendering: **p50 112 ms / p90 163 ms wall**,
  **1.7 / 2.5 frames** at the 15.6 frames the board drew per wall second
  (two heartbeats' frame counts). At `4df5d1b3f`, on a quieter host:
  p50 45 / p90 47 ms, 1.1 / 1.1 frames at 23.8 fps. The board's own
  heartbeat says ≈560–580 fps, which is `t1`'s emulated clock (one cycle
  per instruction), not a rate anyone sees; the cell prints frames at that
  rate too (62.6 / 91.1 here), and they mean nothing.
- **Back after a deploy:** 29.5 s wall (runs at earlier commits: 7.2,
  13.4, 13.5, 20.5, 26.8, 27.3, 27.6, 30.2 and one 47.4 s). The plan's
  "within 15 s" is a wall bound on an emulated run, so it is printed and
  not asserted. The spread is not diagnosed: the board's retry backoff is
  in emulated time and the emulator's pace against wall time varies with
  host load.

## 4. What this does not cover

- **Radio and the internet.** The uplink is a host socket two hops from
  the board; no Wi-Fi air, no NAT, no real DNS, no TLS terminator in
  front of the relay. The desk sitting (P10) is the board's word on those.
- **Studio.** The walk is lp-cli-driven; Studio's relay walk is M8's.
- **Time.** Every time above is wall time on a loaded host; the frame
  figures convert it with a frame rate taken the same way.
- **One board.** The C6 holds one network session; two boards on one
  relay are the host harness's (`lp-cli/tests/relay_link.rs`).

## 5. The run directory

`target/walk-wifi-emu/relay/`: `walk.log` (everything the cell and the
emulated board printed) and `summary.json` (the configuration, each step,
the measured lines).

## 6. Round 2 (2026-10-07): the memory fix, and the gate it is read against

Yona's ruling after round 1 (the plan's "round 2" notes): allocate the
relay's buffers only while the board may dial (a), free what the idle path
holds while the other serves the session (b), and only if the gate case was
still under its floor lower the floor (c). The commits are `ab4b24792` (a),
`17b3e7670` (b), `af4c35ded` (the leg owns its buffers, which brought the core
back under its flash page), and the merges of main `f879d8f4a`, `12a9cbd8c`,
`27ff68db6` and `81816d2f4`. Label for every figure here:
`lp-emu:esp32c6:t1+net=lan@<commit>`, the board's heartbeat; free bytes never
move run to run, the **largest block is bimodal** (host timing moves the
emulated order of allocations), so each cell says how many runs.

**The corrected gate.** Section 3 read the gate case against 16,384 B. That is
the plan's A3/AC8 wording, from the LAN's earlier gate. The firmware's own
`READ_GATE` for the C6 is **40 KiB free / 8 KiB block** (main's read-frame
budget: a read on a fragmented heap shrinks its frames to half the largest
block), so (c), lowering it to 12,288 B, would have raised it and was **not
applied**. Every row below passes the gate the firmware runs, in every run,
including round 1's. AC8's heap bar follows that gate (DD208); 16,384 B
stays as a target the rows now also meet in most runs.

**Heap, by step** (free / largest block, B):

| Row | round 1 (`f879d8f4a`, 4 runs) | after (a) (`ab4b24792`, 1 run) | after (b) (`17b3e7670`, 3 runs) | final (`af4c35ded`, 6 runs) | after main's #1005 (`81816d2f4`, 2 runs) |
|---|---|---|---|---|---|
| joined, no key | 173,504 / 94,276–94,292 | 173,536 / 94,340 | 174,632 / 95,396–95,436 | 174,632 / 95,412–95,444 | 174,568 / 95,340 |
| registered, no session | 166,512 / 87,300–87,332 | 166,544 / 87,328 | 168,732 / 89,492–89,524 | 168,732 / 88,976–89,556 | 168,668 / 89,500–89,508 |
| project + relay session | 56,784 / **13,448** (4/4) | 56,816 / **13,448** | 57,912 / **20,432** (2/3), 13,440 (1/3) | 57,912 / **20,432** (4/5), 13,440 (1/5) | 57,848 / **20,368** (2/2) |
| project + LAN session | 56,684–56,716 / **13,384** (4/4) | 56,716 / **13,384** | 57,848 / **20,424** (1/1) | 57,816–57,848 / **20,424–20,440** (5/6), 13,384 (1/6) | 57,784 / **20,368** (2/2) |

(One final-image relay run, and two (b) LAN runs, printed no heartbeat inside
30 s under host load of about 100 or more, so they are no data.)

**Which step did what.** (a) moved nothing in the gate case: the relay dials
there, so its buffers are held. **(b) moved the largest block from 13.4 KB to
about 20.4 KB**, in most runs and not all (about one run in six stays near
13.4 KB, which is still over the 8 KiB gate). (a) is what a keyless board
needs: its free bytes read identical with Cloud relay on and off on every
image (63,972 after (a); 63,980 after (b) and final; 63,916 at `81816d2f4`),
so a board with no key holds no relay memory. The largest block of that probe
(a network saved, `projects/test/basic` uploaded over `lan:`, a LAN session
open, a scratch test not committed) matched on and off after (a) and (b)
(19,588 / 26,508) and **did not on the final image** (15,880, 15,880, 19,604
on, against 26,508 ×3 off), and matched again at `81816d2f4` (18,172 on and
off): same bytes, a different layout, reported and not tuned.

**What (b) could and could not free.** Neither path's whole set can go: the
LAN listener and its WebSocket buffer must stay to accept a same-key takeover
from the relay and to answer anyone else busy, and the relay leg must stay
registered so a second key through the relay is told busy. What can go is each
path's outgoing frame while the other serves the session (the LAN's 1,088 B,
the relay's 1,091 B). A build with no LAN endpoint at all, the most that
"a relay session frees the LAN" could ever give and one that breaks the
takeover, put the relay-session row at only 16,600 B; it was measured and not
shipped. Takeover relay → `lan:` and busy still pass.

**Flash, measured with `just fw-esp32c6-size-check` on this machine:**

| Build | core | engine @ | gated headroom | to the next core page |
|---|---:|---|---:|---:|
| before round 2 (`f879d8f4a`) | 1,408,560 | 0x170000 | 123,028 | 464 B under |
| (a) + (b) as first written (`17b3e7670`) | 1,410,160 | 0x178000 | 90,250 | 1,136 B over |
| `af4c35ded` (the leg owns its buffers) | 1,408,976 | 0x170000 | 123,018 | 48 B under |
| `12a9cbd8c` | 1,408,960 | 0x170000 | 122,998 | 64 B under |
| `81816d2f4` (after main's #1005) | 1,415,728 | 0x178000 | 89,146 | 6,704 B past the previous boundary |
| CI's build of `a303512e4` | | | **88,308** | |

The core crossed its 32 KiB page in the merge of main's #1005, not in this
PR's code (DD209); Yona accepted the result as "OK but tight". The first
form of (a) + (b) crossed it itself, and `af4c35ded` exists only to get back
under it.

**What round 2 did not change or measure.** The time figures in section 3
(boot to registered, request p50/p90, deploy to back) are round 1's; the cell
still prints them and asserts none. `lp-net`'s stack high water was never
measured (it needs a `net_thread_stack_diag` build), and core-only
registration is not exercised by any emulated cell (no uplink in the split
boot cell). The C6 heap ratchet's figures are CI's, not a local bless:
105,580 B used / 195,956 B free / 116,840 B largest block at boot with
nothing saved.

**Open findings from the walk, filed or left as notes:**

- `lp-cli emu run --elf --reboot-on-reset` never returned after a reset:
  filed (`docs/defects/2026-10-07-emu-run-elf-reboot-on-reset-never-returns.md`,
  open, undiagnosed).
- One relay-restart recovery took 47.4 s of wall time (the other nine 7.2–30.2 s):
  a note here. It is one observation in ten on a loaded host (load average 68 on the last run),
  the retry backoff runs in emulated time while the emulator's pace against
  wall time moves with load, and the bound that matters (15 s) is a desk
  measurement (DD200). Not filed.
- The board's console has no line when its relay state changes: a note here
  and in the cloud-relay ADR. It is a trade (688 B of core crossed a page),
  not a defect; the heartbeat and the status answer carry it.
