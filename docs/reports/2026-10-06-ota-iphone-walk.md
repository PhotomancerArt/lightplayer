# Studio updates a board over Bluetooth: the iPhone/Bluefy walk (protocol)

**Status: walked and passed, 2026-10-07, by Yona.** On loose-c6, with the
merged head of #1005: a full update over Bluetooth with its backup ended up
to date ("it worked! quite impressive"). Not walked on the phone: locking it
mid-update and the engine-less restore, which the desk runs (a cut mid-backup
resumed; a power cut at 164 s) and the emulated walks cover. His notes on the
copy (Finishing always said "It was interrupted"; a Wi-Fi link told him
"Firmware updates need USB") were fixed before the merge. The text below is
the protocol as written before the walk. The gate of
`lp2025/2026-10-05-0820-ota-studio-ble-updates` (M7, PR-3, P13): a person,
an iPhone running Bluefy, and a desk C6, with Studio served from the PR's
branch. **2026-10-07: the director hosts the walk on `loose-c6`** (below);
`fixture-c6` is the agent's Mac Chrome pre-walk board. The agent's Mac Chrome pre-walk (the same flow on the same
board, Mac Chrome as the central over CDP) is in the PR; it is the agent's
evidence that the flow works on a real radio, **not** a phone's number. This
file is filled in at and after the walk: the blank rows below are for it.

## What you need

| | |
|---|---|
| **The board** | `loose-c6` (LC6, "Loose C6", MAC `10:BD:A3:B0:8E:30`), hosted by the director (2026-10-07). It advertises its project's name while its engine runs and `LP-8e30` core-only; an iPhone may show either (iOS caches names). **Keep its USB free of a host during the update** (power only — a USB charger, or the hub with no program holding the port). A host on the board's USB link used to close the Bluetooth update link mid-backup (`docs/defects/2026-10-07-a-usb-host-on-the-board-closes-its-bluetooth-update-link.md`, fixed: run e1 held one for 40 s mid-backup and the link stayed up), but the walk is about the phone, so leave the cable out of it. The agent's pre-walk board was `fixture-c6` (FC6, `A0:F2:62:87:B4:8C`, `LP-b48c`), restored after. |
| **Studio** | This branch's Studio, built and served on the desk by the agent (its own firmware is **Y**, the version the card offers), on the tailnet over HTTPS so Bluefy can open it: `https://<desk>.<tailnet>.ts.net:8443/` — the agent posts the exact URL at the gate. Web Bluetooth needs a secure context; the phone cannot reach `127.0.0.1`. |
| **The phone** | Bluefy, its Bluetooth on, within a few metres of the board. Close other Bluefy tabs that hold the board. |
| **The agent** | At the desk's terminal, to make the board engine-less over USB for step 3 (`scripts/ota/make-engine-less.sh 10:BD:A3:B0:8E:30` on loose-c6) and to read the board's console **between** steps, never while the phone is updating it. It restores the backup after the walk. |

The board holds a password (Play and Author set to Password) unless the
agent says otherwise at the gate; Bluefy asks for it once, on the card.

## The steps

Read the card at every step (step 4 is "throughout"). Each row says what it
should read; anything else is worth a note.

### 1. Update the board to this Studio's version from the card

1. Open the URL in Bluefy. Devices → add a board → **via Bluetooth** → pick
   the board (`LP-8e30`, or its project's name). Unlock with the password
   if the card asks.
2. The card offers the install as a plain button (one click, no confirm):
   **Update** for a release newer than the board's, or **Install dev
   <Y>** when either side is a dev build, which is the case at this walk
   (dev versions have no order, so the card names the version instead).
3. Press it. Expect, in order:
   - **"Backing up current firmware… N%"** — only when no copy of X's
     engine is held (the first update of the sitting); the show keeps
     running;
   - **"Updating over Bluetooth… N%"** — the board's LEDs go **dark
     yellow**; the board resets (the link drops and comes back by itself:
     Bluefy reconnects a held board with no picker);
   - **"Finishing the update… N%"** — the engine; more resets;
   - the card ends on **`<Y> · up to date`** ("…, the same as this
     Studio."), and the project is still running (the LEDs show it, the
     editor opens it).
4. The card's terminal (open the board's details) has one line per reset,
   **"board reset · reconnected in N s"**, and the update's bytes, seconds
   and rate at the end.

### 2. Mid-update, lock the phone (or walk out of range), then come back

1. The agent puts X back first (so there is an update to make): tell it
   when you are ready.
2. Press the install again; once the card says **"Updating over
   Bluetooth… ~30–50%"**, lock the phone (or walk away until the link
   drops) for ~20–60 s; then unlock (or walk back).
3. Expected: if the page survived, the card says **"Finishing the update…"**
   or carries on **"Updating over Bluetooth…"** from where it stopped (the
   board resumes — the agent reads `[OTA] resuming core at …` on its
   console), with no click, and ends on **`<Y> · up to date`**. If iOS
   discarded the page: re-open the URL and pick the board again (DS12: a
   reconnect, not a repair) — the card finishes on its own.
4. **Fails it:** anything that needs USB, a Factory reset, or a reload
   loop.

### 3. An engine-less board restores itself on connect

1. Close the board's card on the phone (or the tab), and tell the agent.
   It runs `scripts/ota/make-engine-less.sh A0:F2:62:87:B4:8C` over USB:
   the board's engine header is erased, and it boots **core-only** (its
   LEDs **dark red**: it needs its engine).
2. Connect to it from the phone again.
3. Expected, with no click: **"Restoring firmware… N%"** (the LEDs dark
   yellow while it writes), a reset, then **`<Y> · up to date`** and the
   project running. Studio holds the engine from step 1's update (its
   engine cache); a fresh Bluefy profile without it would fetch it from
   this Studio's own build.

### 4. Read the card throughout

The words above are the merged spike's (direction C). Note any sentence that
reads wrong, any state the card did not show, any button that should not
have been there (no **Factory reset** while updating, no **Update** while one
runs), and whether the LEDs' colour matched the card's picture slot.

## How long it takes, and what to watch

**Bluefy writes Studio's data frames without response, at most 8 in
flight** (`without-response:8`, the iOS default of the Bluetooth write
policy; desktop Chrome runs `without-response:16`). SYN and ACK-only frames
always go with response, and every frame goes with response while the link
is stalled. The page console (or `?record=`) prints the policy once per
link — `[ble N] bluetooth: data frames without response, at most 8 in
flight` — and a rate line every 15 s:
`[ble N] bluetooth: out … KiB/s, in … KiB/s over 15 s · R of F frames resent (…%) · srtt … ms`.

**Measured on Mac Chrome via CDP (fixture C6, ~1 m, 2026-10-07, the merged
build — not a phone's numbers):**

| Phase | Mac Chrome (16 in flight) |
|---|---|
| Backup (the engine read back, 1.84 MB) — only when Studio has no copy of the board's engine **and** the store has none (a dev build: a board on a published release gets its engine from the release store, never a read-back) | 1016 B pieces since the backup fix: 233–342 s (5.4–7.9 KiB/s on average; 15 s windows up to 9.6–13.5 KiB/s) at host load 33–212, six runs, none ended mid-backup by itself |
| Core (1.37 MB raw, 0.85 MB on the wire) | 22–23 s at best (38 KiB/s on the wire), 85–135 s at worst |
| Engine (1.84 MB raw, 1.09 MB on the wire) | 74–112 s (16–24 KB/s raw) |
| Each board reset → reconnected | 1.0–3.0 s |
| Press → up to date, no backup | **122 s**; **164 s** with a power cut at 40 % of the core |
| Press → up to date, with a backup | **297–507 s** where Chrome reconnected by itself (d1, r1, e1); the other four needed the board re-picked after a board reset (below) |

**Expect on the iPhone:** no Bluefy number exists yet — this walk is the
first. With half Mac Chrome's window, plan on **3–6 minutes** without a
backup and **another 4–8 minutes** if the card starts with "Backing up
current firmware…" (dev builds only; a released board's engine comes from
the store). Keep the phone awake and near the board for step 1.

**What to watch:**

- The card's % **stalling** for more than ~30 s, or rising by a percent
  every several seconds where it had moved every second.
- The console's `resent` share rising past ~10 %, or `srtt` past ~1 s.
- A **drop/reconnect loop** — "reconnecting…" over and over, or a
  reconnect that never comes ("1 remembered board not connected"). On Mac
  Chrome four of eight runs lost the board (after a reset, or a link that
  ended mid-transfer) and never reconnected by themselves, and after the
  backup fix four of seven again, every one at a board reset (into
  core-only after the backup, or onto the trial core); each time,
  re-picking the board from Devices finished the update with no other
  click. Whether Bluefy reconnects by itself is what step 2 asks.
- A backup that ends early, or a backup that does not pick up again after a
  reconnect: fixed on the desk (the backup resumes from its last piece,
  and waits up to 30 s for the reconnected link's login). The board now
  says why it refuses (`[OTA] refused on link N: A: log in first`).

**Fallback URLs** (append to Studio's URL, then reload and re-pick the
board): `?ble-writes=without-response:4` (a smaller window, if frames are
being lost), `?ble-writes=with-response` (every frame with response, #880's
rule: slowest, most conservative).

**Keep the board's USB unplugged from any host** (power only) while it
updates. The defect that made this a must (a host on its USB link closed
the Bluetooth link mid-backup) is fixed; it stays the walk's rule so the
phone is the only thing under test.

## The gate's questions (for Yona)

1. Did the update end on the right version, with the project still running?
2. Did the interrupted update finish (or the board heal) with no repair —
   re-opening Studio and picking the board again counts as a reconnect
   (DS12); anything needing USB, a Factory reset or a reload loop fails?
3. Did the engine-less board restore itself on connect, with no click?
4. Did the card's words make sense at every step, and did the board's lights
   (dark yellow, dark red) match the card's picture slot?

**Pass** = yes to all four. A Bluefy-only failure is a desk finding to fix
and re-walk; a copy note is fixed before merge without a new walk.

## The agent's pre-walk on silicon (2026-10-06, Mac Chrome 154 via CDP, board ~1 m)

The fixture C6, backed up first (two reads agreed, `deec1695…`: the
previous holder had left its own `b1b1b1b1` test build on it, not the
`77a0701d…` the desk expected) and restored after to the desk's known-good
`77a0701d…` (read back equal). X = `a0a0a0a0`, Y = `b0b0b0b0` (split images of
this branch); Studio's own build `7b7c8f326`. Lab password on the board,
`open: nobody`. Every rate is **Mac Chrome via CDP**, not Bluefy. Evidence:
`lp2025/2026-10-05-0820-ota-studio-ble-updates/data/prewalk-2026-10-06/`.

**Part C P3 (`lp-cli link capture blepipe:`, the pipe writes WITHOUT response):**

| Step | Result |
|---|---|
| Refusal, no password | engine answered `N`/`A`; `Stopped(NeedsEngineLogin)` in 10 s; nothing erased (a wrong password: the same) |
| X→Y with password, `Z` | **pass**, 190 s; 1,839,253 B served at 10.3 KiB/s while connected; 3 board resets, reconnects 1.0 / 1.7 / 1.0 s; the core asked its own login and got it; `Z` 743 chunks + 2 raw = 60.3 % of raw |
| Power cut mid-core (Y→X) | **pass**, 156 s; cut at 30 s into the core; back up 0.9 s after the drop; resumed at **778,240 / 1,220,448 B**; converged on X |
| Heal, no login, engine in cache | **pass** (after the fix below): 1,089,806 B in 68.6 s (15.5 KiB/s); reconnect after commit 1.0 s; engine running |
| Five in a row | X→Y 190 s, Y→X 156 s (the cut), X→Y **(1 Chrome restart)**, Y→X **no USB host: 177 s**, X→Y **(1 Chrome restart)**, Y→X 109 s, X→Y 151 s — all converged |
| A backup read back over Bluetooth (engine, before an update with an empty cache) | ~3.4 KiB/s: not finished within the 9-minute run window; the later runs seeded the host's engine cache instead |

**Studio itself (writes WITH response):**

| Step | Result |
|---|---|
| Connect, unlock (typed), card | "Install dev 7b7c8f3", plain; unlock 0.5 s |
| Install, then a power cut at 30 % of the core | **first try: FAILED** — the core-only half refused Studio's login (`LoginRefused`), board left core-only, card stuck on "Finishing the update… 0%" → fixed (credentials) and filed (the card) below. **Second try: pass to the core commit**: "board reset · reconnected in 1.2 s"; the cut at 30 % → "reconnected in 8.0 s", resumed at 30 % with no click; core 1 % ≈ 5 s (≈1.3 KiB/s on the wire), 99 % at ~10 min |
| After the core's commit | Mac Chrome did not reconnect (GATT connect then 0x13); the Update activity's gap ran out and the card left the roster ("1 remembered board not connected"). The board was fine: core confirmed, waiting for its engine |
| Re-pick the board (DS12: a reconnect) | "Restoring firmware… 0 %" at once, no click; engine 1.09 MB in ~288 s (3.8 KiB/s); committed; Studio again lost the board after the last reset (Mac Chrome) while the board ended running `7b7c8f326` |
| Close the tab mid-engine, reopen | not run (the lease and the reconnect behaviour above) |

**Found and fixed in this PR:** core-only dropped the Wi-Fi controller and
took Bluetooth off the air (`docs/defects/2026-10-06-core-only-drops-the-wifi-controller-and-bluetooth-goes-dark.md`);
Studio could not log in to core-only after a typed-password unlock
(`…-a-typed-password-unlock-cannot-log-in-to-core-only.md`). **Open:** the
card holds "Finishing…" after a refused login
(`…-the-card-holds-finishing-after-a-refused-login.md`); Mac Chrome's
reconnect after some board resets (7 Chrome restarts and 1 crash in the
sitting, the known macOS lore) — whether Bluefy reconnects is step 2's
question.

## The agent's speed pass on silicon (2026-10-07, Mac Chrome 154 via CDP, board ~1 m)

Fixture C6, merged build: X = `a0a0a0a0+a3bf56bebe57`, Y = Studio's own
`a3bf56beb+a3bf56bebe57`, Studio the release story bundle of `a3bf56beb`.
Backed up first (`305d3f97…` by the file and the chip's own MD5), restored
after (on-chip MD5 `305d3f97…` again), power-cycled, lease dropped. **Wi-Fi
on, 0 networks saved, station `notConnected` in every run** (not joined).
No USB host unless the row says so. Host load in brackets. Evidence:
`lp2025/2026-10-05-0820-ota-studio-ble-updates/data/ble-speed-2026-10-07/`.

| Run | Policy | USB host | Backup | Core | Engine | Press → up to date | Resent (frames) | Reconnects |
|---|---|---|---|---|---|---|---|---|
| b1 before (backup, stopped at 400 s) | with-response:16 | no | 0→98 % in 401 s, 7.9 decaying to 2.4 KiB/s [33–105] | — | — | stopped by design | 50 of 3,833 | 0 |
| a0 after, resumed (same board, new policy) | without-response:16 | no | (done by b1's page) | 84 s, 5–21 KiB/s | ~90 s, 38 → 1.6 → 26 KiB/s | ~183 s from the join that worked (2 joins timed out first; 1 Chrome restart) | 151 + 96 | 1 reset |
| a1 full, engine cached from b1 (no backup) | without-response:16 | no | skipped | 22 s (38.6 KiB/s wire) | 74 s, one drop mid-engine | **122.1 s** [134–146] | 311 | 1.1, 1.0, 2.0 (drop), 2.0 s |
| a1d full with backup | without-response:16 | no | 11.8 → 2.6 KiB/s; link ended at **58 %** (200 s), no reconnect [100–140] | — | — | **failed** | 4 of 2,270 | none |
| a1e full with backup | without-response:16 | no | 6.5 → 1.9 KiB/s; link ended at **59 %** (258 s), no board reset, no reconnect [37–100] | — | — | **failed** | 1 of 2,343 | none |
| a1f full with backup | without-response:16 | no | **7–11 KiB/s steady, 99 % in 233 s** [33–65]; board reset into core-only, Chrome connected then dropped (0x13), no retry | — | — | **failed at the core's start** | 11 of 3,872 | 0 of 1 |
| a2 seeded (no backup) | without-response:16 | no | skipped | 135 s, 2–13 KiB/s, srtt to 2.2 s | to 74 %, link ended, no reconnect; a re-join finished 75→100 % in 27 s | not in one go | 177 + 174 + 29 | 3.0, 2.0 s |
| a3 power cut at 40 % of the core | without-response:16 | no | skipped | 23 s (58 KB/s raw) | 112 s (16 KB/s raw) | **163.9 s** [18–61] | 444 | 2.0 (cut), 2.0, 1.3 s |
| s1 backup with a USB host | without-response:16 | **`lp-cli link capture`** | 1.7 KiB/s, **board closed the link at 17 s** (`reply deadline`, 1584 ms of the tick's budget left); reconnected 2.0 s; `[OTA] refused` ×4; stuck at 0 % | — | — | **failed** | 7 of 101 | 2.0 s |

Surprise 1 (a USB host stalls the Bluetooth backup) **still holds** on the
merged build, now as a closed link instead of a stall: s1 against a1f. Filed
as `docs/defects/2026-10-07-a-usb-host-on-the-board-closes-its-bluetooth-update-link.md`.

## The backup fix on silicon (2026-10-07, Mac Chrome 154 via CDP, board ~1 m)

Fixture C6, X = `a0a0a0a0+81077deeeef7` (refusals and resets in words),
Studio the release story bundle of `ed15e1091` (1016 B read-back pieces,
the engine-login wait) for r1–c3 and of `6f3fc621b` (the wait on any later
link) for e1; Y = Studio's own build. `--fresh-cache` every run (the engine
cache really empty). Wi-Fi on, 0 networks, not joined. No USB host unless
the row says so. Backed up first (chip MD5 `305d3f97…` = the earlier
backup file), restored after. Host load in brackets. Evidence:
`lp2025/2026-10-05-0820-ota-studio-ble-updates/data/ble-backup-2026-10-07/`.

| Run | Backup (to 99 %) | Press → up to date | Mid-backup link ends | Resent (frames, links ended) | Reconnects | Result |
|---|---|---|---|---|---|---|
| d1 (before the piece fix, `81077deee`) | 211 s, 8 KB/s [64–169] | 297.7 s | 0 | 242 of 15,352 | 2.0, 1.0 s | complete, no click |
| d2 (before the piece fix) | link closed by the board at 35 % (232 s): `reply still not out of the frame buffer after 4633 ms` | — | 1 | 5 of 1,406 | 2.0 s, refused ×4, resumed; then stuck at 36 % (a lost answer) | the diagnosis run |
| r1 | 318 s, 5 KB/s [213–168] | **507.2 s** | 0 | 268 of 15,889 | 1.3, 2.0, 3.0 s | complete, no click |
| r2 | 270 s [150–108] | core done at ~318 s; Chrome did not reconnect after the reset onto the trial core; re-picked → up to date in 72 s | 0 | 75 of 9,342 | 1.0 s | complete, **re-pick** |
| r3 | 342 s [91–14] | Chrome did not reconnect after the reset into core-only; re-picked → up to date in 76 s (two joins before the press hung until a Chrome restart) | 0 | 36 of 4,355 | — | complete, **re-pick** |
| c1b | 233 s [33–56] (the cut did not fire: Mac Chrome has no `getDevices`) | Chrome did not reconnect into core-only; re-pick: core, then a drop mid-heal; a second re-pick found it up to date | 0 | 22 of 4,347 | — | complete, **two re-picks** |
| c3 cut at 40 % at the central | 323 s [59–24]; cut at 144 s, reconnected 2.0 s, backup went on; the new link ended again 13 s later (cause not seen), reconnected 2.0 s, went on | Chrome did not reconnect into core-only; re-picked → up to date in 82 s | 1 cut + 1 | 28 of 4,436 | 2.0, 2.0 s | **resumed**; complete, **re-pick** |
| e1 USB host 40 s from 30 % (the old s1) | 326 s [32–113]; the link stayed up; board frames `max ≤ 105 ms` while the host was on | **426.5 s** | 0 | 248 of 15,832 | 3.0, 2.0 s | complete, no click |

Three consecutive full updates with a backup (r1, r2, r3) completed; r2 and
r3 needed the board re-picked after a board reset, which the brief counts
apart. The cut run (c3) resumed the backup twice with no click. A cut by
the central is `gatt.disconnect()` on the page's own GATT server: the board
sees a clean disconnect, not an out-of-range timeout.

## Measured at the walk (fill in)

Central: **Bluefy on iPhone (model, iOS version: ______)**, board ~___ m
away. Every rate below is this central's, never Mac Chrome's.

| Row | Value |
|---|---|
| Step 1 — update time (press → up to date) | |
| Step 1 — Studio's rate line (bytes, s, KB/s) | |
| Step 1 — reconnects, each time ("reconnected in …") | |
| Step 1 — did it back up first? | |
| Step 2 — locked or walked away? for how long? | |
| Step 2 — page kept or discarded by iOS? | |
| Step 2 — resume offset (`[OTA] resuming … at …`, board console) | |
| Step 2 — time from return to up to date | |
| Step 3 — time from connect to "Restoring firmware…" | |
| Step 3 — restore time, reconnects | |
| Board console excerpts (agent) | |

## Yona's answers (fill in)

| Question | Answer | Notes |
|---|---|---|
| 1. Right version, project running | | |
| 2. Interrupted update finished, no repair | | |
| 3. Engine-less board restored, no click | | |
| 4. Words and lights made sense | | |

## After the walk (agent)

- Restore the board's backup and read it back (`77a0701d…79aa`); the
  restored image answers its link only after a power cut
  (`board power-cycle fixture-c6`).
- `tailscale serve --https=8443 off`.
- Fill the tables above; fix copy notes; a Bluefy-only failure is fixed and
  re-walked; then the PR goes ready and `yona-ship` takes it to the ship gate.
