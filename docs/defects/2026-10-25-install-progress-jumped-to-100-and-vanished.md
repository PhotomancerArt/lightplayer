---
status: fixed
found: 2026-10-25      # how: Yona, installing firmware on a blank ESP32-C6 from a phone in Studio
area: Studio — the board card's install progress (lpa-devices activity cell, FlashStep; lpa-studio-core board surface)
class: state-conflation   # one "latest progress" slot held every step and every log line
related:
  - lp-app/lpa-devices/src/activity/activity_cell.rs (ActivityCell::percent, land_progress)
  - lp-app/lpa-devices/src/activity/flash_step.rs (FlashStep::shows_percent, Restarting, SavingBoardModel)
  - lp-app/lpa-devices/src/device.rs (Device::forward: the percent ends with its effect)
  - lp-app/lpa-studio-core/src/app/devices/board_card/board_surface.rs (installing_sentence)
  - lp-app/lpa-devices/tests/scenarios.rs (an_install_counts_only_the_write_and_names_each_wait)
---
# The install's progress jumped to 100 %, then vanished, then sat silent

**Symptom** — Yona installed firmware on a blank C6 from a phone. The card
said "Installing the firmware on this board." at **100 %** almost as soon as
the install started. Then the percent went away. Then the card showed no
percent, and no new words, for a while. Then it said installed.

**Cause** — four separate things, all in how the card read progress:

1. **A step's percent outlived its step.** The card's percent was the
   latest progress event on the activity cell, and nothing cleared it when
   the flash moved on. So the percent from one step stayed up through the
   next one until that step reported for the first time.
2. **The first step finishes at once.** "Reading the board's layout"
   (`browser_esp32_flash.js`, `inspectLayout`) reads about 3 KB, so it
   reaches 100 % almost immediately. Because of (1), that 100 % then stayed
   up while the firmware package loaded, the loader started and the baud
   rate changed, all before the write's first tick.
3. **Log lines erased the percent.** `browser_transport.rs` forwards every
   esptool log line as progress with a label and no percent, so the
   terminal can show the narration. On the cell, each of those lines
   replaced the write's percent with none. During the write the number
   flickered off and back between ticks.
4. **The waits after the write had no words.** After "Resetting the
   board", the card kept saying "Installing the firmware on this board."
   over a spinner, with no percent, while the model waited for the new
   firmware's hello and then wrote `/hardware.json`. Neither wait was named.

**Fix**

- The percent ends with the effect that counted it. `Device::forward`
  clears the cell's progress when its current effect ends and when a new
  one starts.
- A progress line with no percent is narration once a step is counting
  (`ActivityCell::land_progress`). It still goes to the journal and the
  terminal, but it leaves the step's label and percent on the cell. Before
  the first percent arrives, it is still the best label there is.
- Each `FlashStep` says whether it shows a percent
  (`FlashStep::shows_percent`). Reading the board shows one only for a read
  of the board's files, which takes real time. The few-kilobyte layout read
  shows none. Waiting for the user's answer, restarting and saving the
  board model have nothing to count, so they show none either.
- Two new steps: `Restarting` (the flasher's closing reset and the wait for
  the hello, on an install that moves no files) and `SavingBoardModel` (the
  `/hardware.json` write). The card's sentence follows the step
  (`installing_sentence`): "Reading this board before installing.",
  "Installing the firmware on this board." with the write's percent, "This
  board is restarting on its new firmware.", "Saving the board model on
  this board."

**Pinned by** `an_install_counts_only_the_write_and_names_each_wait` in
`lpa-devices/tests/scenarios.rs`, which replays the phone install through
the model: no percent before the write, no gap between the write's ticks,
and a named step at every point. A second test,
`an_install_says_each_step_and_counts_only_the_write` in
`board_surface.rs`, renders the card at each step. The stories
`board_card_surface_install` and `devices_card_update_steps` show the steps.

**Lesson** — a value shown on screen should say which thing it is about,
and stop showing when that thing is over. A "latest progress" slot shared
by every step and every log line will eventually show one step's number
under another step's words.
