# ADR: The Picked Pattern Is a Bus Setting

- **Status:** Accepted
- **Date:** 2026-10-20
- **Deciders:** Photomancer (Yona's rulings are the vision's D1–D16; the
  plan's Q1–Q12 and DP1–DP11 are recorded in
  `lp2025/2026-10-20-0001-playlists-on-the-bus`)
- **Supersedes:** the argument of `docs/design/panel.md` P12 (the pick as a
  command); `docs/adr/2026-07-27-runtime-node-command-channel.md` (the
  channel had one command, and it is gone)
- **Superseded by:** None

## Context

On the PLAYFUL choker tryout, from a phone: pause the clock, turn the cycle
off, pick Aurora, pull the power. It came back paused, with the cycle off,
playing Soft Noise. The pause (`clock.play_state`) and the cycle
(`playlist.cycle`) were panel writers, saved in `.lp/panel.json` and
restored before the first frame (panel.md P10/P11). The pick was a runtime
command, `WireNodeCommand::PlaylistActivateEntry`, that nothing saved.

And turning the cycle off did not hold: a held cycle fell back to the
playlist's older timed walk, and the tryout's entries still carried
`"duration": 30`
(`docs/defects/2026-10-19-the-cycle-switch-off-does-not-hold.md`).

panel.md P12 had kept the pick a command for three reasons: a panel write
would be "a writer with no value to hold, a persisted 'current entry' that
fights the cycle, and a Clear that means nothing". None holds up. The writer
holds an entry key. What persists is the entry the cycle walks *from*, not
the cycle's position; the cycle never writes it. Clear means "rest on the
authored entry again".

## Decision

1. **The picked entry is a setting on the bus** (vision D1). `PlaylistDef`'s
   `idle_entry` is consumed from `bus:playlist.entry` with `panel = "show"`,
   the way `cycle` and `skip` already were. The authored value is the
   default; a pick is a panel write that holds and persists; Clear returns
   the authored entry. `playlist.json` bytes do not change.
2. **A picked entry holds** (D7). The playlist moves on its own in exactly
   two ways: the cycle, which walks the enabled entries from the picked
   entry; and a trigger's excursion, which plays the triggered entry's
   `duration` and comes back to the picked entry (while cycling, the cycle
   carries on where its clock says). A `duration` means only that. The
   chained timed walk is gone. A triggered entry with no duration stays
   until something else switches (a pick, a step, another trigger, or the
   cycle's next step).
3. **A fresh write is a new pick, even of the same value** (Q2). Re-tapping
   the entry the cycle started from restarts the cycle there. Each panel
   write carries a write identity (`PanelWriter::write`), which a consumer
   reads with `TickContext::consumed_slot_panel_write`. A writer that is a
   node counts on a changed value only.
4. **Boot loads the restored pick** (Q3). After panel restore, and after a
   structural reattach, the engine hands each node the panel writers that
   supply its load-deciding slots
   (`NodeRuntime::load_deciding_slots`/`settle_consumed_panel_value`). A
   playlist that has not ticked yet makes the restored pick its selected
   entry and asks for it, so the first tick's residency step swaps the
   authored entry out before anything renders: it is never sampled or
   compiled. A pick naming no authored entry plays the authored entry.
5. **A Button's next/prev is a runtime override** of the pick (Q9): a new
   pick drops it, a reboot forgets it (D12, until hardware inputs come in as
   panel writers, panel.md P13).
6. **Next/previous is one rule**, `lpc_model::playlist_step`, used by the
   engine's next/prev trigger ids and Studio's next/prev offers (D15).
   Studio steps from a pick still on its way, else from the entry playing.
7. **The runtime node-command channel goes** (D15, DP2).
   `PlaylistActivateEntry` was its only command. `WireProjectCommand::
   NodeCommand`, `WireNodeCommand`, `NodeRuntime::handle_command`,
   `Engine::handle_node_command` and the client methods are deleted;
   `WIRE_PROTO_VERSION` 44.
8. **What's playing is on the bus, read-only** (D11):
   `PlaylistState::active_entry` is produced onto `bus:playlist.playing`.
9. **One playlist per module** (D10): two playlists reading one scope would
   share its pick, cycle and skip list. The engine warns at load and
   Studio's save check names the second.
10. **Studio** presses a pick as `PlaylistPickOp` (a panel write that keeps
    load-to-edit's card focus), and a new `reset` offer lets the pick, the
    cycle and the skip list go together (D13). Turning the cycle off also
    picks the playing pattern, so "hold this" survives a reboot (the cycle
    itself never writes the pick).

## Consequences

- **The Play tier can now pick, next and previous.** A panel write needs
  Play; the command needed Edit. That is the play tier the wire-version-skew
  vision's D10 describes, and the vision's "no playlist verb anywhere": the
  agent, the board card, Bluetooth's play tier and a future stable panel
  surface all speak panel writes.
- **Wiring can drive the pattern.** A time-of-day or audio node that writes
  `playlist.entry` picks the pattern; the phone takes it over (P5 jump) and
  Clear hands it back. `docs/design/modules.md` §10's "data-driven playlist
  activation" is ordinary wiring now.
- **Boot still pays the authored entry's load** (derive and attach, then its
  unload in the first tick), because the registry decides residency before
  panel state exists and has no persist-path mapping to read it with. No
  compile and no frame are spent on it. Measured on the PLAYFUL tryout,
  `lp-emu:esp32c6:t1` (lp-emu at `c2212c0fd`, emulated, never
  hardware-validated; `lp-cli/tests/emu_playlist_pick_reboot.rs`): first
  light 258 ms with a restored pick against 172 ms on the authored entry,
  the difference all in the `lit` stage (139 ms against 52 ms) — the
  authored entry's unload and the pick's load. Seeding the registry's
  resident set from `panel.json` before derivation would save it; it is
  the first follow-up.
- **A pick made less than 10 s before a power cut can be lost**: panel state
  is written at most every 10 s (P11 as built).
- Button steps are not remembered across a reboot.
- `PlaylistNode` no longer accepts runtime commands, and no node does.

## Alternatives Considered

- **The playlist owns its position** (the server saves it next to the panel
  state, and every input is an event). It would remember button steps and
  where a tour was. Rejected: a new kind of saved record, wiring can't drive
  it, and the panel would grow playlist verbs.
- **Keep the command, persist it on the side.** Two paths to one setting,
  and still no wiring and no Clear.
- **Seed boot residency from `panel.json` before derivation** (no wasted
  load). The registry knows no tree paths and nothing of panel state; it
  would need the panel's persist-path rule duplicated. Deferred, see
  Consequences.

## Follow-ups

- Patterns declare what they share (`module.json` `inputs`) and the panel
  band: the same plan's Part B.
- Hardware inputs as panel writers (panel.md P13) make button steps
  remembered.
- Seeding boot residency from the restored pick, if the measured cost of
  the authored entry's load at boot matters.
