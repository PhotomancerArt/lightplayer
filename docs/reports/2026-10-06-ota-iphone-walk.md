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
| **The board** | `fixture-c6` (FC6, XIAO C6 on its LED panel, MAC `A0:F2:62:87:B4:8C`), on the desk, USB plugged into the hub (power and console only). Its Bluetooth name is `LP-b48c`. The agent has backed it up (`77a0701d…79aa`), flashed **X** (`a0a0a0a0`, this branch's split image) and left it running with its project. **Never** the loose XIAO that advertises `LP-8e30`. |
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
2. The card's firmware line reads **`a0a0a0a0 → <Y> available`** (Y is
   this Studio's version, shown in the header popover) and offers
   **Update** as a plain button (one click, no confirm).
3. Press **Update**. Expect, in order:
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
2. Press **Update** again; once the card says **"Updating over
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
