# Presets lab

A toy model of presets, activations and schedules, built to test the model
against the requirements before any product code exists. It sits outside the
app on purpose, like `lab/mapping`. The discussion that produced it is in the
planning workspace: `lp2025/2026-10-17-0305-wall-clock-schedules/` (`notes.md`
for the decisions, `model.md` for the model written out).

```bash
cargo test -p lab-presets-model           # the statements
open spikes/presets-and-schedules/index.html   # the behaviour, to play with
```

## The model

The panel's value for each lighting control comes from a stack of layers,
bottom to top, last wins:

1. **Project** — what the author set.
2. **Panel defaults** — the user's baseline, a preset that never ends. A
   house keeps power off here; its schedules turn the lights on.
3. **Active presets** — in the presets' own priority order.
4. **Overrides** — what turning a control changes. Temporary: the layer ends
   at the schedule's next change.

A **preset** pins some controls and nothing else. An **activation** holds a
preset on, from one **source**, for a duration (or until turned off); there
is at most one per (preset, source). A **schedule** is a trigger: each
firing makes an activation, its end worked out at the firing. Editing a
schedule never touches what it already turned on.

Where a board would keep each part: the project's files; **saved** (panel
defaults, presets, schedules — flash); **live** (activations, overrides,
deleted firings, the log — RTC RAM, which survives a soft reset or an
update's reboot and not a power cut).

The same tick that fires a schedule catches up on a firing the board was off
for: a preset is turned on by a schedule when its latest firing in the
schedule's current form is still running and nobody deleted it.

## What it leaves out

Timezones and daylight saving (local time is passed in), the real sun
(`src/sun.rs` is a smooth stand-in for Seattle), transitions between values,
inputs (buttons), and the board itself. `model/` depends on nothing.

## Statements

`model/tests/statements.rs` holds the behaviour as sentences, one test each,
tagged with the requirement they cover. Change the sentence first, then the
code.
