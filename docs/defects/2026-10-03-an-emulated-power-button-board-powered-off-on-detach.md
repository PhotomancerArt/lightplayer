---
status: fixed
found: 2026-10-03      # how: live-debugging (Yona, reviewing PR #937 on ?emu=)
fixed: this change
area: lpa-studio-web public/lpa-link (virtual_serial.js, emulator_picker.js, index.html)
class: stand-in-divergence
related:
  - docs/defects/2026-10-02-a-dropped-link-sends-the-editor-to-devices.md
  - lp-core/lpc-engine/src/nodes/power_button/power_button_node.rs
---
# An emulated board running a power-button project powered off the moment its cable was detached

**Symptom** — testing PR #937 (a dropped link keeps the editor) on
`?emu=`, a board running a project with a switch-mode `PowerButton`
(`button:local:D0`, the PLAYFUL choker) never came back after the
banner's `detach`. The serve log said
`guest entered deep sleep (ext1 wake: gpio0 high)`, `GET /boards` said
`state: stopped`, and `attach` failed with
`emulated board c6-a: control channel is not open` until `emu serve` was
restarted. It cost a debugging round, because it looked like #937's
reconnect failing.

**Root cause** — the firmware was right: a switch reading "off" keeps the
board awake only while a USB host is attached (`power_button_node.rs`),
so with the cable out it powers off. The switch read "off" because the
emulated board's D0 is a pad nothing drives, and an undriven pad reads
low. On the real choker that pad is wired to a switch that is on whenever
someone is wearing it. The emulated board stood in for the board without
standing in for what is wired to it, and the emulator models no
deep-sleep wake, so the stand-in's difference was terminal. A second,
smaller gap made any page-side fix fragile: a chip restart puts the whole
pin fabric back to its power-on snapshot, outside drives included
(`Esp32C6Machine::restore_in`), so a `pin 0 1` sent once is gone after
the next reset or flash.

**Fix** — the dev banner holds a per-board **D0 switch**, **on** by
default. `createBus({ holds: { 0: true } })` (passed only by the `?emu=`
page) sends the emulator's own `pin 0 1` verb at load, on the switch,
after `attach` and after every reboot the port observes. Before `detach`
it reads the pads (`pins`), re-sends a hold a restart dropped, and gives
the guest 500 ms of its own time to debounce a recent change before the
host goes. Flipping the switch off and detaching powers the board off, as
the firmware intends. Both backings (`emu serve`, `?emu=tab`) go through
the same `EmulatorPort.command()`. The tab's outcome table also gained
`7: "deep-sleep"`, which it had been reporting as `"7"`.

**Regression coverage** — `just walk-drop-emu` now walks the PLAYFUL
Choker by default: both cable pulls (under the editor and under Play)
ride out with the switch on, and a last step flips it off, pulls the
cable and waits for the board's registry row to read `stopped` /
`deep-sleep`. Not a CI job, like every walk. The conformance suite still
pins that Studio's own calls send no verb of the shim's: holds are opt-in
and the suite passes none.

**Lesson** — an emulated board is a chip plus whatever its project
assumes is wired to it. When a project reads an input, the emulated
board needs a stand-in for that input with a sensible resting state, or
the firmware will correctly act on a level no real board would show.
