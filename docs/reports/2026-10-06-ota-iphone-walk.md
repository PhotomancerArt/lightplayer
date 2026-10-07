# Studio updates a board over Bluetooth: the iPhone/Bluefy walk (protocol)

**Status: protocol, not yet walked.** The gate of
`lp2025/2026-10-05-0820-ota-studio-ble-updates` (M7, PR-3, P13): a person,
an iPhone running Bluefy, and the desk's fixture C6, with Studio served from
the PR's branch. The agent's Mac Chrome pre-walk (the same flow on the same
board, Mac Chrome as the central over CDP) is in the PR; it is the agent's
evidence that the flow works on a real radio, **not** a phone's number. This
file is filled in at and after the walk: the blank rows below are for it.

## What you need

| | |
|---|---|
| **The board** | `fixture-c6` (FC6, XIAO C6 on its LED panel, MAC `A0:F2:62:87:B4:8C`), on the desk, USB plugged into the hub (power and console only). It advertises `LP-studio` while its engine runs and `LP-b48c` core-only; an iPhone may show either (iOS caches names). Before the walk the agent backs it up (the desk's known-good image is `77a0701d…79aa`), flashes **X** (`a0a0a0a0`, this branch's split image), adds the walk's password over USB and leaves X running its project. **Never** the loose XIAO that advertises `LP-8e30`. |
| **Studio** | This branch's Studio, built and served on the desk by the agent (its own firmware is **Y**, the version the card offers), on the tailnet over HTTPS so Bluefy can open it: `https://<desk>.<tailnet>.ts.net:8443/` — the agent posts the exact URL at the gate. Web Bluetooth needs a secure context; the phone cannot reach `127.0.0.1`. |
| **The phone** | Bluefy, its Bluetooth on, within a few metres of the board. Close other Bluefy tabs that hold the board. |
| **The agent** | At the desk's terminal, to make the board engine-less over USB for step 3 (`scripts/ota/make-engine-less.sh A0:F2:62:87:B4:8C`) and to read the board's console. It restores the backup after the walk. |

The board holds a password (Play and Author set to Password) unless the
agent says otherwise at the gate; Bluefy asks for it once, on the card.

## The steps

Read the card at every step (step 4 is "throughout"). Each row says what it
should read; anything else is worth a note.

### 1. Update the board to this Studio's version from the card

1. Open the URL in Bluefy. Devices → add a board → **via Bluetooth** → pick
   `LP-b48c`. Unlock with the password if the card asks.
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

## Expect it to be slow, and why

Studio writes every Bluetooth frame **with response**, one at a time
(#880's rule), so it moves far less than the desk pipe does. In the Mac
Chrome pre-walk (below) the core went at ~1.3 KiB/s on the wire and the
engine at ~3.8 KiB/s: about 10 minutes for the core and 5 for the engine,
and a first update that must back the engine up first (no copy of the
board's engine on the phone) reads ~1.8 MB back first — on the order of 10
more minutes. Bluefy is a different stack and may be faster or slower; the
walk measures it. Keep the phone awake and near the board during step 1.

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
