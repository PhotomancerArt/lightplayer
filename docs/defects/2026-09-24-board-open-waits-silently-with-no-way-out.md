---
status: fixed
found: 2026-09-24      # how: report (Yona, JSON Pack desk sitting, PR #795, real XIAO C6)
fixed: this change
area: lpa-studio-core open_progress × StudioController device-lens hold × lpa-studio-web project_opening_frame / web_app route sync
class: assumed-context
related:
  - docs/adr/2026-09-22-opening-a-board-adopts-its-project.md
  - docs/defects/2026-09-10-the-emulated-c6-builds-a-graphics-stage-40x-slower-than-silicon.md
---
# Opening a project on a board waited silently, with no way out

**Symptom** — two reports from one sitting on a real XIAO ESP32-C6 running
PLAYFUL Choker:

1. "projects take a very long time to load with little feedback, and in one
   case it seems maybe the device crashed. no way to reset on the loading
   page though, I had to refresh browser, reset, go to device page."
2. Loading `/p/playful-choker-prj…?on=mac:a0:f2:62:87:b4:8c` fresh hung.
   The console said, and then said nothing more:

       [studio] waiting for the device before opening it: missing session: this board is not connected
       [studio] Waiting for the device

Both reproduce on the emulated C6 (`just studio-dev-emu`, headless Chrome,
the imported `catalog/projects/playful-choker`). The shim's in-page picker
starts every fresh page with no grants, the same as Brave. The page stayed
on "Opening project…" for 45 s and more, with no button. A board reset
while the project was being sent sent the page to `/devices` after ~7 s. It
showed no message; the reason ("device did not respond within 5.0s") was
only in the console.

**Root cause** — three things the opening frame could not see:

- *The hold assumed the board would arrive by itself.* An open on a board
  that is not ready is held (`pending_device_lens`) and the tick retries it.
  The code said so: "Silicon is untouched: a board is on a desk, and
  waiting for it is the correct behaviour." That is true for a board that
  is identifying. It is false for a board this page holds no port for:
  `requestPort()` needs a user gesture, and nothing on the page offered
  one. The hold also leaves the actor free, so `user_open_in_flight` was
  false and the frame fell back to its calm "Opening project…" skeleton.
- *A board open had no stages.* The frame narrates the engine download
  and the worker boot, and a board has neither. The stop, the upload, the
  `LoadProject` (the board compiles every shader here) and the read-back all
  showed as "Preparing the project…" or as nothing.
- *The route sync threw the failure away.* When the editor goes away, the
  view→route loop in `web_app.rs` sends the URL to `/devices` ("the open
  ended"). It did not check whether the open ended in a FAILURE, so the
  failure notice (message and Retry), which only a project address renders,
  was unmounted before it painted.

**Not a firmware crash.** The capture Yona pointed at
(`lp2025/2026-09-23-1701-lp-json-pack/g1-150.bin`) has one
`[io_task] server frame USB write timed out at chunk 1/1 (0 of 172 B) after
250 ms` followed by `dropping message id=0`. That is a heartbeat
(broadcast id 0), dropped while Studio was not reading (`responses=0`).
The board kept rendering at 30 fps (`frame=20823` → `20955` over the next
heartbeat). Every heartbeat in the session's captures reports `level:
green`, `lastCrash: null`, and no blame paths. The one reboot in the
set (`g1b-packed-150.bin`) is `rst:0x15 (USB_UART_HPSYS)`, a reset driven
over USB from the host, not a panic or a watchdog. The firmware's follow-up
Error frame ("response id=0 dropped") is read by the client as unsolicited
and ignored, so it cannot wedge an open either. What *looked* like a crash
was the open waiting with nothing on screen.

**Fix** —
- `open_progress` gains `OpenStage::WaitingForDevice { device, reason }`
  (not connected / port closed / identifying / busy / unknown) and
  `OpenStage::OnDevice { device, step }`: connecting → clearing → uploading
  (acknowledged bytes of total) → loading → reading back. A failure while
  the open is on a step names the step
  ("… stopped while sending the project to the board: …").
- `LpClient::replace_and_load_project_observed` reports each deploy step.
  `StudioServerClient::open_library_project` forwards them, and they only
  land while the stage is `OnDevice`, so a sim's open is unchanged.
- `cancel_open()` supersedes the open and wakes the device request it is
  parked on. The device request deadline is raced against it.
  `RuntimeOp::CancelOpen` then drops the hold, the lens and the pending
  package.
- The frame shows the board and the step, a real upload bar, and after 8 s
  on one step, how long. It always offers Cancel and (with a port) Reset
  the board. For `NotConnected` it offers **Connect this board** (a
  Reconnect, which asks the chooser). The failure notice offers Reset the
  board and goes back to Devices.
- `web_app.rs` keeps a failed open on its address, so the notice renders.

**Regression coverage** —
`studio_device_e2e_tests::a_fresh_page_names_the_board_it_cannot_reach`,
`::connecting_the_board_a_held_open_waits_on_lands_the_open`,
`::cancelling_a_held_open_lets_the_board_go`; `open_progress::tests::
a_failure_on_a_board_names_the_step_it_was_waiting_on`,
`::a_sim_open_never_narrates_board_steps`,
`::cancel_supersedes_the_open_and_wakes_its_parked_request`; the frame's
stories (`board_not_connected`, `board_not_answering`, `board_uploading`,
`board_loading_stalled`, `failed_on_board`). The route-sync change has no
unit test (it lives in the page's view loop); the emulator walk in PR #818
covers it.

**Lesson** — a wait that only something outside the page can end must put
that something on the page. "The tick will retry" is a plan for a board
that is coming. It is not a plan for one that cannot come until someone
clicks.
