---
status: fixed
found: 2026-10-06      # how: hardware-walk (the FC6 silicon re-check, N7, kit ec48b7afc)
fixed: this change (silicon re-check owed)
area: lpa-server load gate (`PROJECT_LOAD_MIN_HEADROOM_BYTES`) × first-fit placement on the C6 heap
class: rounded-measurement-at-threshold
related:
  - docs/adr/2026-10-07-project-loads-are-tried-and-recovered.md
  - docs/defects/2026-08-29-load-project-resets-instead-of-refusing.md
  - docs/defects/2026-10-06-a-refused-project-switch-leaves-the-board-dark.md
  - docs/defects/2026-10-07-the-emulated-c6-cleared-rtc-fast-memory-on-every-reset.md
  - lp2025/2026-10-05-1903-wifi-link-c6 (PR B, #989)
---
# A Wi-Fi-joined C6 refuses every project switch

**Symptom** — FC6 re-check on silicon (kit `ec48b7afc`). With Wi-Fi
joined, every project switch was refused by the 64 KiB load gate:

- LAN link open: largest free block 46,444 B.
- LAN, USB and BLE open: 38,652 B.
- USB only, no LAN client ever: refused too. Small islands split the main
  region, and a 6.9 KB block sat in `dram2_seg`, the 64 KiB region that
  could otherwise pass the gate on its own.

With Wi-Fi off, the switch passed only because `dram2_seg` was empty. The
main region's largest hole was 54 KB even then.

**Root cause** — two things together.

1. **The gate.** It wanted one 64 KiB block, sized for a 5,950-lamp
   dome. Measured in the emulator (byte-exact for memory) with
   `heap_track_diag` and the new `[bigalloc]` log, a 73-LED choker's
   largest single ask during a load is 8 KB. Small Dome's largest is its
   47,600 B lamp list.
2. **Placement.** The heap is first fit, so whatever is allocated while a
   project runs and outlives it splits the space the project frees (the
   2026-09-24 class). These blocks were named in the emulator:
   - the link mux's per-link lists, reserved after the boot project
     loaded;
   - a packing host's 6.9 KB learned table, allocated at opt-in;
   - the station's strings, on every state change;
   - the FPS window's sample buffer, reallocated every heartbeat;
   - a LAN link's session, made on the net thread when a client connects.

The emulator also showed what the gate never caught. Small Dome loaded,
then ran the board out of memory on its first frame (a 20 KB ask), and
since it had already become the startup project it reset the board on
every boot.

**Fix** — no gate. A load is tried and recovered
(`docs/adr/2026-10-07-project-loads-are-tried-and-recovered.md`):

- It is recorded in the RTC recovery region until its project has run 3
  frames.
- A switch becomes the startup project only then.
- A board that resets in between boots the previous project and says so
  ("Small Dome didn't fit in memory — back on PLAYFUL Choker", in the
  heartbeat's `load_notice`, on Studio's card and in its journal).
- A startup load that reset it is not tried again.
- An out-of-memory load blames nothing in the ledger.
- The lamp list is reserved fallibly.

Placement was fixed where it was cheap:

- the mux is built before the boot project;
- the FPS window prunes in place;
- `heap_map_diag` now logs through the link and arrives whole.

A context-routed allocation arena (the net thread's allocations into
`dram2_seg`) was prototyped, and with it the emulated switch cleared
64 KiB by 7–10 KB. It is held, because without the gate it is not needed
for loads.

**Proof (emulated, `lp-emu:esp32c6:t1+net=lan`, this change)**

- Choker → Small Dome → `basic`: Small Dome reset the board once. It
  came back on the choker, saying "Small Dome didn't fit in memory —
  back on PLAYFUL Choker", ledger green. `basic` then loaded.
- Small Dome alone on a blank board: two resets, then idle, saying
  "Small Dome didn't fit in memory at startup, so no project is
  running". No loop.
- Wi-Fi joined, LAN link open: choker → `basic` → choker → Small Dome →
  `basic` all switched with no reset. Small Dome's lamp list was refused
  in words ("5950 lamps need 47600 B in one piece, more than this
  board's memory has free").
- Not fixed here: after the switch to `basic` with the LAN link open,
  the read gate refused reads (largest block 16,164–16,172 B < 16,384).
  The read gate is left as it is for now, and Yona wants it revisited the
  same way.

**Regression coverage**

- `lp-recovery`:
  - `a_switch_that_ran_out_of_memory_is_an_interrupted_load_not_a_blame`
  - `a_startup_load_that_reset_the_board_is_skipped_next_boot`
  - `a_finished_load_or_a_users_reset_is_no_interrupted_load`
  - `a_load_that_panicked_is_interrupted_and_still_blamed`
- `lpa-server` (`tests/project_load_tried.rs`):
  - `a_load_is_tried_whatever_the_headroom_probe_says`
  - `a_switch_becomes_the_startup_project_after_its_first_frames`
  - `a_failed_switch_leaves_the_previous_project_running`
- `lpa-devices`: `a_load_that_did_not_fit_is_said_in_the_boards_words`

**Lesson** — a gate guesses a project's cost before reading it. It
refused loads that fit and let through the ones that did not. On a small
board, trying and recovering across the reset is both cheaper and more
honest, as long as a load counts as done only when its project has
actually run.
