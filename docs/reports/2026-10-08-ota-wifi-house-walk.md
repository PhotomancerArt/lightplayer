# Studio updates a board over Wi‑Fi: the house walk, LAN half (protocol)

**Status: walked and passed, 2026-10-08, by Yona** (see "The result"
below; the steps for the lid-closed and second-tab cases were not walked).
Gate G1 of
`lp2025/2026-10-06-2249-ota-wifi-updates` (PR B, P8). Yona, at home, with a
board on the house Wi‑Fi and **no USB cable**. The agent's pre-walk (below)
ran the same flow on the desk, on the test access point, with Mac Chrome
headless: it is evidence the flow works on a real radio, not the feel of it.
The blank rows below were not filled in; the result is recorded in prose.

## The result (2026-10-08, Yona)

**Passed.** Board: loose-c6, on the house Wi‑Fi. Studio: this PR's, in a
laptop browser. Power was cut at about 50 % during each of the three
stages: **the backup, the core update and the engine update**. All three
recovered with no lost progress. In Yona's words: "it worked perfectly. I
killed power at 50% on each stage. backing up firmware. first stage update,
and second stage update. it recovered all three times without even losing
progress."

| Step | Walked? |
|---|---|
| 1. Studio reaches the board over Wi‑Fi | yes |
| 2. Update | yes |
| 3. Update again, power off mid-update | yes (power cut at about 50 % in the backup, the core and the engine) |
| 4. Close the laptop (or the tab) mid-update | **not walked** |
| 5. A second Studio tab while an update runs | **not walked** |

Per-step timings and card wording were not recorded.

## What you need

| | |
|---|---|
| **The board** | A C6 on this branch's firmware (the split image, PR A's board half), on the **house** Wi‑Fi, running a project, powered through something you can switch that is **not** a USB host: a USB charger on a switch or smart plug, or the PLAYFUL choker's own switch. The agent prepares it over USB before you leave the desk: flashes **X** (`target/walk-ota-emu/images/x/package/fw-esp32c6-merged.bin`, a dev build `a0a0a0a0`, flashed with the package image so the board's files stay), saves the house network from Studio's Wi‑Fi panel, uploads a project. |
| **Studio** | This branch's Studio on the Mac, on the house network: `LP_FW_IMAGE=split just studio-dev` (a few minutes to build; the URL it prints is the one to open). Its own firmware is **Y**, the version the card offers. Served from `localhost`, so Chrome asks no Local Network question; the release Studio at lightplayer.app would (not this walk). |
| **The way in** | The board, met over USB once in this browser, is remembered with its Wi‑Fi address: its tile offers **Connect over Wi‑Fi**. Or type its address (`lp-xxxx.local` or its IP, on the board's Wi‑Fi panel) into the add slot's **board's address**. |
| **For step 3** | X's update files, to install X back: `target/walk-ota-emu/images/x/ota/` (Other version… → From a file…). |

## The steps

Read the card at every step. Note the time of each update (a phone stopwatch
is fine: press to "up to date").

### 1. Studio reaches the board over Wi‑Fi

1. Open Studio. Connect over Wi‑Fi (or type the address).
2. Expect: the card's device line reads **"Wi‑Fi · <address>"**, and the
   firmware zone offers **Install dev <Y>** (two dev builds have no order,
   so the card names the version instead of "Update").

| Result | |
|---|---|

### 2. Update

1. Press **Install dev <Y>**. Expect, in order:
   - **"Backing up current firmware… N%"** (the first update only; the show
     keeps running);
   - **"Updating over Wi‑Fi… N%"**: the board's LEDs go **dark yellow**; the
     board resets, its link drops and comes back by itself;
   - **"Finishing the update… N%"**: more resets;
   - **"dev <Y> · same as this Studio"**, the project still running.
2. The card's terminal has one **"board reset · reconnected in N s"** line
   per reset and the bytes, seconds and rates at the end.

| Time (press → up to date) | Words that read oddly | Project still running? |
|---|---|---|
| | | |

### 3. Update again, and switch the board off for ~5 s in the middle

1. **Other version… → From a file…**, pick X's `ota/` folder, install it
   (it arms first: a custom build). Then install **dev <Y>** again.
2. When the card says **"Updating over Wi‑Fi… ~40%"**, switch the board's
   power off, count five, switch it on. Touch nothing else.
3. Expect: the card keeps the update's line; the board comes back on Wi‑Fi
   by itself, the update resumes where it was ("link dropped during update
   at N%", then "reconnected"), and ends on **dev <Y>** with the project.

| Ended on | Time | Notes |
|---|---|---|
| | | |

### 4. Close the laptop (or the tab) mid-update for ~30 s

1. Install X back (as in step 3), then dev <Y> again.
2. Mid-update ("Updating over Wi‑Fi…" or "Finishing the update…"), close the
   lid (or the tab) for ~30 s; open Studio again (the same URL).
3. Expect: the board either finished by itself and reads **dev <Y>**, or
   is mid-transfer and Studio **finishes it with no click** ("Finishing the
   update…" with the interrupted sentence). A board without its engine is
   **restored** with no click ("Restoring firmware…").

| What the card said on return | Ended on | Notes |
|---|---|---|
| | | |

### 5. A second Studio tab while an update runs

1. Start an update (X back, then dev <Y>).
2. While it runs, open a second tab and **Connect over Wi‑Fi** (or type the
   address).
3. Expect: the second tab says **"Busy with another connection — try
   again"** and does not flicker; the first tab's update finishes.

| Second tab's words | First tab finished? |
|---|---|
| | |

## The questions

- Did every run end on the right version (or on the old one, running, after
  Studio restored it) with nobody touching the board beyond its power
  switch?
- Was the project still there?
- Did the card's words make sense at each step, including "busy" in the
  second tab?
- Was the time acceptable?

**Pass:** yes to all, with no cable and no manual repair.

## The agent's pre-walk (2026-10-08)

FC6 fixture-c6 (`A0:F2:62:87:B4:8C`), the **test access point** (never the
house network), Studio's release bundle of PR B's tree (`f66f2bb85`) served
on `localhost` to **headless Chrome on the Mac**, the board at its LAN
address (`?lan=ws://<ip>/link`), **no host on its USB port** during the
updates. X = `a0a0a0a0+f66f2bb8501a`, Y = `f66f2bb85+f66f2bb8501a`. Mac
Chrome numbers, not a phone's and not the house network's.

| Run | What | Press → up to date | Stages (Studio's terminal) |
|---|---|---|---|
| r1 | X → Y, empty engine cache (backup read back) | **78.7 s** | backup 1 841 KB in 31 s (58 KB/s) · core 1 424 KB in 12 s (118 KB/s) · engine 1 841 KB in 15 s (119 KB/s); 3 resets |
| r2 | X → Y, X's engine cached, board power-cycled at "Updating over Wi‑Fi… 40%" (`board power-cycle`) | **50.4 s** | core in 12 s (115 KB/s) · engine in 15 s (119 KB/s); "link dropped during update at 43%", resumed, ended on Y |

- For comparison on the same desk (PR A, lp-cli over `lan:`): X → Y with a
  backup 58.8 s; USB 31.0 s.
- Every reconnect line read **"reconnected in 1.0 s"**, the power cut's
  included: over Wi‑Fi that line does not measure the board's time away
  (not yet explained). Use the press-to-done time.
- Chrome's Local Network prompt: not observable headless, and not expected
  for a page on `localhost`.
- The board was backed up first (SHA-256 `77a0701d…79aa`) and restored
  exactly after (read back, same hash). Records:
  `lp2025/2026-10-06-2249-ota-wifi-updates/data/desk/p8-*`.

The emulated walk (`just walk-ota-emu --lan`, `lp-emu:esp32c6:t1+net=lan`)
covers the same flow step by step: update, power cuts mid-core and
mid-engine, an engine-less board restored, a renumbered board, and lp-cli
plus a second Studio turned away busy.
