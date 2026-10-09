# Studio updates a board through lightplayer.app: the relay walk (protocol)

**Status: walked and passed, 2026-10-08, by Yona.** loose-c6 updated to
`2026.10.08-10` through lightplayer.app's relay; he unplugged it mid-update
and it resumed without incident. The steps below are the protocol he walked
from; the rows were not filled in, and this is the record. Gate G2 of
`lp2025/2026-10-06-2249-ota-wifi-updates` (PR C, P10). A walk of its own:
the Wi‑Fi roadmap's relay walk passed, but before this walk no update had gone through
the relay on a real board. About 30 minutes.

## Before you start: what must be true

1. **This PR is on lightplayer.app.** The relay is the page's own origin
   (`/relay/board/<mac>`, with your sign-in's cookie), so only a Studio
   served by lightplayer.app can reach it; a local `just studio-dev` cannot.
   The relay half is still behind `?relay=`, so nothing changes for anyone
   who does not type it.
2. **The board runs a core from this PR.** loose-c6 (LC6) runs
   `dev a48e9c8` (PR B's build). Its core turns relayed links away, so its
   first update has to happen **nearby**: through the relay, Studio would
   end the update after 5 s with **"Update nearby once"**. Step 0 does it.
3. **The board holds your account's key.** It does since the Wi‑Fi relay
   walk (Studio put it there over USB while you were signed in). If the
   card through the relay says it can't be reached with your keys, plug
   it in over USB once while signed in.
4. **You can switch the board's power** without a USB host: a switch, a
   smart plug, or the PLAYFUL choker's own switch.
5. **A release newer than the one step 0 installs.** An update needs
   somewhere to go, and only releases from this PR's merge on carry the
   relay-capable core. Every merge to main publishes one, so this is
   usually a matter of hours; the card then says **"<version> available"**.

## Step 0 — nearby, once: put this PR's core on the board

At home, Mac on the house Wi‑Fi, signed in at `https://lightplayer.app`.

1. Connect the board over Wi‑Fi (its card's **Connect over Wi‑Fi**) or USB.
2. Install the release this PR's merge produced (**Update** if it is the
   latest; otherwise **Other version…** and pick it). Expect what G1 saw:
   backing up, updating, finishing, up to date, project still running.

| Release now on the board | Notes |
|---|---|
| | |

## Step 1 — leave the board's network

Put the Mac on your phone's hotspot (or take the phone on cellular, step 5).
Stay near the board's power switch; that is fine, only the network must
differ. Close any Studio tab still on the board over Wi‑Fi.

## Step 2 — reach the board through lightplayer.app

Open `https://lightplayer.app/?relay=<the board's MAC, 12 hex digits>`,
signed in.

Expect: the card's link line says **"Wi‑Fi via lightplayer.app"**, and the
firmware zone offers **Update** (to the newer release) and **Other
version…** — **not** "Firmware updates need USB" (what it said before this
PR).

| Card's link line | Firmware zone's offer |
|---|---|
| | |

## Step 3 — update through the relay

Press **Update**. Note the time from the press to "up to date".

Expect, in order: **"Updating over Wi‑Fi… N%"** (dark yellow LEDs), three
board resets, each followed in the card's terminal by **"board reset ·
reconnected in N s"**, then **"Finishing the update…"** and up to date, the
project still running. Each reset drops the board off lightplayer.app for
a few seconds; the page waits for it.

| Time (press → up to date) | Words that read oddly | Project still running? |
|---|---|---|
| | | |

## Step 4 — power off mid-update

Go back first: **Other version…**, pick step 0's release, confirm (it is
older, so it arms; it may also warn that it "needs a nearby update after,
via lightplayer.app" — that warning is wrong for a release from this PR on,
and stays until the first such release is named in Studio). Then **Update**
again, and at **"Updating over Wi‑Fi… ~40%"** switch the board's power off,
count five, switch it on. Touch nothing else.

Expect: the card keeps the update's line ("link dropped during update at
N%"), the board rejoins Wi‑Fi, reaches lightplayer.app again, the update
resumes where it was, and ends up to date.

| Ended on | Time | Notes |
|---|---|---|
| | | |

## Step 5 — the network drops mid-update

Go back to step 0's release again, then **Update**. Mid-update, turn the
hotspot off (or the phone to
airplane mode) for ~15 s, then back on; reopen Studio if the tab closed.

Expect: the update finishes by itself, or the board is mid-transfer and
Studio finishes it with no click ("Finishing the update…"); a board left
without its engine is restored ("Restoring firmware…").

| What the card said on return | Ended on | Notes |
|---|---|---|
| | | |

## Optional — the iPhone

Safari on the iPhone, on cellular, signed in to the same account: open the
same `?relay=` URL and update. Or start an update on the iPhone and finish
it from the Mac (the same key takes the board's one network slot over, so
the other device's tab loses it).

## The questions

- Did every run end on the right version (or on the old one, running,
  after Studio restored it), with nobody touching the board beyond its
  power switch?
- Did the words make sense through the relay?
- Was the time acceptable? (The relay's leg moves 2 KiB per round trip, so
  expect slower than the LAN's ~60 s.)
- Optional: did finishing from the other device work?

**Pass:** yes to the first two, with no cable and no manual repair.

## What is already proven, and where

- **Emulated** (`just walk-ota-emu --relay`, `lp-emu:esp32c6:t1+net=lan`,
  a local lp-cloud-server standing in for lightplayer.app, headless Chrome):
  X → Y with a backup through the relay (3 resets; backup 21 s, core 30 s,
  engine 26 s, wall, emulated); the relay dropping the board's leg mid-core
  (one "board offline" ridden through, back in 4.0 s, the core resumed at
  its record); a power cut mid-engine (resumed). Not the internet, not
  fly's proxy, not a NAT, not a radio.
- **The agent's desk pre-walk, through a LOCAL relay** (2026-10-08; FC6
  fixture-c6 `A0:F2:62:87:B4:8C` on the desk's test access point; an
  lp-cloud-server on the Mac with a made-up dev account, the board's images
  built to dial it, `LP_RELAY_HOST`; Studio's release bundle of this PR's
  tree in headless Chrome on the Mac; no host on the board's USB during the
  update): X → Y with a backup and the board's power cut at "Updating over
  Wi‑Fi… 41%", **104.2 s** from the press to up to date (backup 41 s at
  45 KB/s, core 24 s at 60 KB/s, engine 23 s at 81 KB/s; three resets and
  the cut, each "reconnected"); the core resumed at its record. Silicon and
  a real radio, but **not** lightplayer.app: no internet, no fly proxy, no
  NAT. The real relay needs a real account's sign-in, which is yours. The
  board was backed up first and restored exactly after (SHA-256
  `77a0701d…79aa`). Records: `lp2025/2026-10-06-2249-ota-wifi-updates/data/desk/p10-*`.
- **Host tests**: the anonymous key refused through the relay, a play key
  refused a core install even on a board open to anyone, an edit key's
  install taken (`lpc-update`, `fw-esp32-common`'s `core_only_links`,
  `lpa-server`'s `access_gate`); a held relay session redialling through
  "board offline" (`lpa-link`'s WebSocket conformance suite, Firefox).
- **Not proven before this walk:** the real relay's round trips, a home
  router's NAT across the board's resets, how long the board takes to come
  back to lightplayer.app after a reset on silicon.
