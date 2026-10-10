# Pictures through the cloud, walked on an emulator: relay protocol 2, a protocol 1 core, and the crossing update

**Date** 2026-10-09 · **Plan** `lp2025/2026-10-08-2050-pictures-through-the-cloud`
(P6, PR #1066) · **Commands** `just walk-wifi-emu relay`,
`just walk-wifi-emu relay-p1`,
`WALK_OTA_X=target/walk-ota-emu/images/x-p1 just walk-ota-emu --relay --steps update`
(and `--steps relay-drop`) · **CI cell** `lp-cli/tests/emu_relay_link.rs`
(in `test-emu-serve`) · **Silicon twin** none yet: the frame-rate cost is
queued for a desk sitting (section 6)

> **STATUS: PASSED.** The relay walk 13/13, the protocol 1 lane 6/6, the
> crossing walk's `update` and `relay-drop`. Every heap row is over the C6
> read gate (40 KiB free, 8 KiB largest block). Everything here is
> emulated: no number in this record is a silicon number.

## 1. What ran, on what

| Run | Configuration | Firmware | Hub |
|---|---|---|---|
| The relay walk (`just walk-wifi-emu relay`, the cell's main test) | `lp-emu:esp32c6:t1+net=lan@d1efe5028` | the shipped single image (`FwImage::SHIPPED`) built from `d1efe5028` (`LP_EMU_BUILD_FW=1`), which says `firmware d1efe5028` in its hello | in-process `lp-cloud-server` from the same tree |
| The protocol 1 lane (`just walk-wifi-emu relay-p1`) | `lp-emu:esp32c6:t1+net=lan@fbcdc12b8` | **CI's** shipped image of `f5039fb93`, the last relay protocol 1 commit (`LAST_PROTOCOL_1` in the lane script): run 37905412888, artifact `ci-images-esp32c6`, `tree/ESP32C6_SERVER_RADIO/fw-esp32c6`, sha256 `fdf81485…5ae58d`; the board says `commit=f5039fb93c46` at boot | in-process `lp-cloud-server` from this tree |
| The crossing walk (`walk-ota-emu --relay`, headless Chrome, Studio's release bundle) | `lp-emu:esp32c6:t1+net=lan`, lp-emu `7bef0e91b`, tree `d1efe5028` | X: **built here** at `f5039fb93` in a throwaway worktree (`scripts/ota/build-image.sh … a0a0a0a0`), `split.json` buildId `a0a0a0a0+f5039fb93c46`; Y: this tree's split package, `d1efe5028+d1efe5028ea4` | a real `lp-cloud-server` from this tree (dev sign-in, mem store) |

The last protocol 1 commit is `main` at this branch's merge base when the
walk ran (`f5039fb93`, after `origin/main` was merged in). CI's images of
it carry no OTA directory, which is why the crossing walk's X was built
rather than fetched (plan Q19).

The firmware sources did not change between `d1efe5028` and the PR's
later commits in this phase (tests, scripts and docs only), so the relay
walk's image is this branch's firmware.

## 2. The relay walk: steps and the board's words

The cell checks each step with its own assertions; the walk then reads the
board's own words in its log. The board's console reaches a host only
over its USB link, so every step that waits for the board's words holds
one (the cell's `UsbConsole`) while it waits.

| Step | What | The board's words, quoted from the run | |
|---|---|---|---|
| R1 | joined, no account key | answers `relay noAccount` | ✓ |
| R2 | registered by itself after a reboot | `[relay] leg open to lightplayer.app`; answers `relay connected` | ✓ |
| **P1** | the hub lists it at `relayProto` 2 with its hello's firmware, no project; it holds the first picture (nothing loaded: 0 lamps) | `[relay] state=connected routes=0 rx=44 tx=95 · takeovers 0 busy 0 · pictures 1 idle`; the cell: `listed at relayProto 2, firmware d1efe5028, no project`, `the hub holds the board's picture (seq 1791545667901559, 0 lamps, 0 samples)` | ✓ |
| R3 | a session through the relay, an upload of `projects/test/basic` | `[relay] route N: link linkN, secure session opening` | ✓ |
| **P2** | the project's name, members only, and its colours | the cell: `the hub lists the project "Basic"; Bob and a guest read no picture`, `the picture carries the project's colours (seq …561, outputs [80], 80 samples, 80 lit)` | ✓ |
| **P3** | a member watching (`BoardPictures { watch: true }` every 2 s): fast, then idle by itself once nobody asks | `[relay] state=connected routes=0 rx=23433 tx=141695 · takeovers 0 busy 0 · pictures 5 watched`, then `[relay] state=connected routes=0 rx=25129 tx=156839 · takeovers 0 busy 0 · pictures 42 idle`; the cell: `watched: seq …559 → …563 in 6.2 s wall` | ✓ |
| **P4** | the heap while watched, with a relay session open | the heap row below | ✓ |
| R4 | the same key takes the session on the LAN | `closed (taken over by the same key)` | ✓ |
| R5 | another key is told busy | `the network link is in use — busy` | ✓ |
| **P5** | lost at a deploy, back with the board | the cell: `right after the deploy the new hub holds no picture`, `a picture is back with the board after the deploy (seq 1791545829716394, 80 lamps)` | ✓ |
| R6 | Cloud relay off, then on | answers `relay off`, then `relay connected` | ✓ |
| **P6** | kept while offline | the cell: `offline, the hub keeps the board's picture (seq …394, online false)`, `back online, the picture is online again and newer (seq …394 → …395)` | ✓ |
| R7 | the account key reset, a deploy: refused | answers `relay refused: unknownAccount` | ✓ |

What the picture steps show, in the board's own count: `pictures 1 idle`
right after registering (the picture the hub's first `PictureRate` asked
for); `pictures 5 watched` a heartbeat after the watch began; `pictures 42
idle` once the lease ran out on the board's own clock — 37 pictures while
watched, about two a board-second over the ~20 board-seconds the lease and
a heartbeat last. No rate is asserted in seconds (AGENTS.md); "more than
one picture while watched, and the board said `watched`" is.

The seq numbers are large because the hub starts each process's count from
its clock, so a deploy's seqs keep increasing.

**R4 needed a fix** (`docs/defects/2026-10-09-the-relay-walk-lost-the-takeover-line-to-the-console-ring.md`):
the unmodified cell failed it on this tree too (`[LINK] 68 log records
dropped`, no takeover line), because the board's 4 KB console ring had
overwritten the line by the time the next USB link opened. Step 4 now
holds a USB link through the takeover and closes it before its heap row.

## 3. The protocol 1 lane: a fielded core at the new hub

`just walk-wifi-emu relay-p1` (the cell's second test, filtered by name,
`LP_RELAY_P1_ELF` set; output in `target/walk-wifi-emu/relay-p1/`). One USB
link is held from the board's first boot to the end, so every line it says
reaches the test.

| Step | What | Evidence | |
|---|---|---|---|
| Q1 | network and Alice's key over USB; registers by itself | `[relay] leg open to lightplayer.app`; answers `relay connected` | ✓ |
| Q2 | listed as it was | `listed at relayProto 1, no firmware, no project` | ✓ |
| Q3 | routed | `a relay session opened at the edit tier, five status requests answered, closed`; the board: `[relay] route 1: link link1, secure session opening (1024 B frames)` | ✓ |
| Q4 | a member watching it for a minute: no picture | `watched for 60 s wall (28 reads): the hub never held a picture for it` | ✓ |
| Q5 | **never sent a protocol 2 frame** | exactly one `[relay] leg open` line across the window; its status answered `connected` 29 times; its heartbeat said `[relay] state=connected routes=0 rx=917 tx=3072 · takeovers 0 busy 0` nine times — **`rx=917` never moved**: while a member watched it, the hub sent the board nothing at all | ✓ |
| Q6 | a deploy: back by itself, still protocol 1, on one new leg | `back after a deploy, still relayProto 1, on one new leg` (two `leg open` lines in all) | ✓ |

A protocol 1 client closes its leg on any frame it does not expect and dials
again, which prints a second `leg open`: one line across the window is the
board's own evidence. The in-process hub is a debug build, whose send guard
also fails outright on a protocol 2 frame for a protocol 1 board.

The test is in CI's cell file but returns at once there (`emu_relay_link
(protocol 1): not asked — LP_RELAY_P1_ELF unset`); the lane is not CI.

## 4. The crossing walk: a protocol 1 core updated through the relay

`walk-ota-emu --relay` with `WALK_OTA_X` pointing at X built at `f5039fb93`.
Studio (the release bundle, `?relay=`) updates the board through a local
lp-cloud-server's relay; after the update the step now waits for Y's
heartbeat to say `pictures N` and reads `BoardPictures` and `ListBoards`
from the page.

**`update` — passed.** The board's words, in order:

```text
[INIT] fw-esp32 initialized, starting server loop... proto=41 commit=f5039fb93c46 dirty=false
[relay] state=connected routes=0 rx=37 tx=82 · takeovers 0 busy 0          ← X: protocol 1 (no pictures)
[OTA] core confirmed
[OTA] engine verified, committing
[INIT] fw-esp32 initialized, starting server loop... proto=41 commit=d1efe5028ea4 dirty=false
[relay] state=connected routes=1 rx=19735 tx=49269 · takeovers 0 busy 0 · pictures 1 idle   ← Y
```

and the walk's line: `Install dev d1efe50; updating → backing up →
finishing → up to date; project kept; X's engine cached; 3 reconnects
timed; then pictures: … the relay holds seq 1791545074011292 (outputs [21],
21 samples), relayProto 2, firmware d1efe5028`. Backup 1,844 KB in 14 s,
core 1,429 KB in 11 s, engine 1,848 KB in 26 s (wall, emulated, a local
relay). This is AC9: a board on the last protocol 1 core updates through
the relay to the new core and then sends pictures.

**`relay-drop` — passed** with the same protocol 1 X: `cut at Updating over
Wi‑Fi… 40% (1 device leg cut); the board back on the relay; [OTA] resuming
core at 581632; up to date; 4 reconnects timed`.

## 5. Numbers

**Heap**, from the board's heartbeat, on the relay walk's run
(`lp-emu:esp32c6:t1+net=lan@d1efe5028`), beside the C6 read gate
(40,960 B free / 8,192 B largest block), with `main`'s figures from CI's
own run of the same cell (`main` `f5039fb93`, run 37905412888, job
`Emulator C6 (x64)`) beside them. CI's run of this PR (`f83e61991`, run
37926434285) read every row below to the byte, except the registered
row's largest block (94,680 B on CI), which moves by a few dozen bytes from
run to run.

| State | `main` (CI) | This branch: free | Largest block | Over the gate |
|---|---|---:|---:|---|
| joined, no account key (no relay buffers) | 181,344 / 102,172 B | 181,160 B | 101,988 B | yes |
| booted with network and key saved, relay registered, no session | 175,444 / 95,588 B | 174,360 B | 94,736 B | yes |
| `projects/test/basic` loaded, relay registered, a relay session open | 64,496 / 18,832 B | 63,300 B | 17,636 B | yes |
| **`projects/test/basic` loaded, relay registered, pictures watched, a relay session open** (P4) | — (no such step) | **63,232 B** | **17,644 B** | yes |
| `projects/test/basic` loaded, relay registered, a LAN session open | 64,432 / 18,824 B | 63,236 B | 17,636 B | yes |

Relay protocol 2 costs a registered board **1,084–1,196 B of free heap and
1,196 B of its largest block** against `main` (the picture buffer, 836 B,
held while the leg is up, and the project's facts); with no account key it
costs 184 B.

Being watched costs 68 B of free heap (63,300 → 63,232 B, the same run's
two relay-session rows) and nothing of the largest block. The unmodified
cell on the same firmware, run back to back, read the same figures for the
rows the two cells share (63,300 / 17,636 B with a relay session, 63,236 /
17,636 B with a LAN session): the new steps do not move them. The largest-block figures are
bimodal from run to run on this cell (P5's run read 26,092 B for the same
row; the relay ADR's "Measured, emulated" says why); every reading here is
over the gate.

**Flash** (P5's spend, copied here so it reads in one place; the split
image, `just fw-esp32c6-size-check`'s figures):

| Build | Core | Engine | Gated headroom | Core → next 32 KiB page |
|---|---:|---:|---:|---:|
| CI, `main` at `f5039fb93` (run 37905412888, job 113744752183) | 1,426,368 B | 1,845,046 B | 88,266 B | 15,424 B |
| CI, this PR's merge commit `2a42c672f` (run 37917190206, job 113777068565) | 1,429,648 B (+3,280) | 1,848,476 B (+3,430) | 84,836 B | 12,144 B |
| local, `main` at `f5039fb93` (X's `build-image.sh`) | 1,426,000 B | 1,844,270 B | — | 15,792 B |
| local, `d1efe5028` (Y's split package) | 1,429,280 B (+3,280) | 1,847,682 B (+3,412) | — | 12,512 B |

The engine still starts at `0x178000`: no page crossed. (`main` itself
grew the core by 1,872 B between `078e77007` and `f5039fb93`, which is why
the page distance before this PR is 15,424 B, not the plan's 17,296 B.)

**The CI cell's wall time** (Q20; wall time, not a gate). **On CI**
(job `Emulator C6 (x64)`, which runs it in `test-emu-serve`): `main`
`f5039fb93` `finished in 112.01s` (run 37905412888), this PR `f83e61991`
`finished in 220.24s` (run 37926434285): **+108 s**, under the plan's
two-minute line but more than the "about a minute" Yona accepted. Most of it
is the board's lease running out on its own clock (`idle again by itself
49.9 s wall after the last watch` on CI). On this host (M2 Max, other
worktrees building), back to back: the unmodified cell `finished in
125.44s` (load average 7–15), the new one `finished in 190.86s` (load
7–37), about +65 s; an earlier pair under heavier load read 157 s and
282 s. Under load the emulated board's clock runs well behind wall time
(the 15 s lease and a heartbeat took 32.8–49.9 s wall to lapse).

## 6. What this does not prove, and who does

- **The frame-rate cost on silicon** — the radio budget's idle row
  (≤ 10 % fps, p99 frame ≤ 100 ms). Emulated time is never a gate. Queued
  in the roadmap's `director-log.md` under "When hardware is at hand"; it
  does not block this PR.
- **The radio**: no air, no signal, no driver heap or timing.
- **A real NAT or resolver, and the internet**: the uplink is a host
  socket; the relay is a local `lp-cloud-server`, not lightplayer.app
  behind fly's proxy. The post-deploy smoke (protocol 1 and 2 hellos to the
  live hub) is the first live check.
- **Studio drawing the picture**: nothing draws it yet (M7, or the
  director's follow-on). The crossing walk reads `BoardPictures` from the
  page with `fetch`, not from any card.
- **Persistence across a deploy**: by design the cache is memory only
  (D4); P5 above shows it is lost at a deploy and refilled by the board.
- **A protocol 1 core on silicon at the new hub**: the lane runs CI's
  emulated image of the last protocol 1 commit; fielded boards are the
  same bytes, but no desk board was walked.

## 7. The run directories

- `target/walk-wifi-emu/relay/` — `walk.log`, `summary.json` (steps, heap
  rows, picture lines).
- `target/walk-wifi-emu/relay-p1/` — the same for the protocol 1 lane, with
  the image's provenance in `summary.json`.
- `target/walk-ota-emu/relay-2026-10-09T11-24-17-956Z/` (`update`) and
  `relay-2026-10-09T11-27-42-632Z/` (`relay-drop`) — `walk-ota-emu.json`,
  the board's console (`console/update/c6-update.link.log`), the relay's
  log, screenshots.
