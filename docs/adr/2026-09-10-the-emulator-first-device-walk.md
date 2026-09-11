# ADR: The emulator-first device walk is the norm; hardware checks parity

- **Status:** Accepted
- **Date:** 2026-09-10
- **Deciders:** Yona (G2, emulator plan two), Photomancer
- **Amends:** the walk practice in `AGENTS.md`; builds on
  [2026-09-09-studio-device-stack-over-a-virtual-serial-port.md](2026-09-09-studio-device-stack-over-a-virtual-serial-port.md)
  and [2026-09-06-esp-soc-emulator-architecture.md](2026-09-06-esp-soc-emulator-architecture.md)
- **Supersedes:** None

## Context

Two plans made an ESP32-C6 device walk possible with no board: the
open ESP32-C6 emulator (`lp-emu/`, ADR 2026-09-06) runs the shipped
firmware and grades its own gaps, and the Web Serial shim (plan two, ADR
2026-09-09) lets Studio's real browser device stack drive an emulated
board through a `navigator.serial` polyfill. An agent can now flash,
connect, identify, upload, detach and re-attach entirely in a headless
browser (`just walk-no-board`,
`docs/reports/2026-09-10-studio-walk-with-no-board.md`).

That capability needs a standing rule, or every planning agent
re-decides from scratch whether a change needs the desk. This ADR is
that rule. It exists because Yona ruled it into being at G2 (2026-09-10):
*"it should probably be written down. We want the emulator-driven walk
to be the norm for agents validating device work."* The wording below is
his, quoted, with the two riders at the end marked as the director's
proposals rather than his.

## Decision

### 1. The emulator-driven walk is the norm for validating device work

Yona, verbatim (G2 Q1):

> yes, that's the goal here. And it should probably be written down. We
> want the emulator-driven walk to be the norm for agents validating
> device work. They should ask for real hardware only if it seems likely
> that what they're working on will be affected by the non-emulatable
> seams. When doing real hardware walks, we should almost always do an
> emulator walk first, and the main thing we're checking with hardware
> is _parity_. If hardware doesn't behave like the emulator, we should
> _fix that_ before fixing the underlying issue so that future walks get
> the more accurate behavior. Thats basically the whole point of the
> emulator.

So, operatively:

- **Default to the emulator walk.** Reach for real hardware only when
  the work is likely affected by a non-emulatable seam (the trigger list
  is rule 3).
- **When a hardware walk is warranted, do the emulator walk first**, and
  the thing hardware checks is **parity** — not "does it work" but "does
  it behave the way the emulator said".
- **Parity failures are fixed in the emulator first.** If hardware
  disagrees with the emulator, the emulator gap is fixed *before* the
  underlying issue, so that every later walk inherits the more accurate
  behaviour. That is the whole point of the emulator.

**Amendment 2026-09-11:** the walk now has a **server-less form**,
`just walk-no-board --tab` — the identical six steps against the C6 emulator
hosted in a Studio tab's own Worker, with no `lp-cli emu serve` process
anywhere. See
[2026-09-10-the-c6-emulator-runs-in-the-tab.md](2026-09-10-the-c6-emulator-runs-in-the-tab.md).

### 2. Emulated measurements are valid, but must never claim to be hardware-validated

Yona, verbatim (G2 Q2):

> that's right. emulated traces, measurements, etc, _are_ valid for
> validation of most firmware things. performance, memory usage, etc.
> unless we have a reason to believe the emulator is wrong. But those
> measurements _must not_ claim to be hardware-validated. they should
> call out the emulator being used, and the version, too (git hash,
> whatever).

So:

- **Emulated traces and measurements are valid evidence** for most
  firmware claims — performance, memory usage, boot behaviour — unless
  there is a reason to believe the emulator is wrong (rule 4).
- **They must not be presented as hardware-validated.** Every emulated
  measurement names the emulator and its version (the `lp-emu` commit
  hash) inline. The model is M6's own trace-provenance line
  (`configuration=lp-emu:esp32c6:t1`, and see the trace ADR
  [2026-09-10-an-emulator-captured-trace-is-evidence-not-a-fixture.md](2026-09-10-an-emulator-captured-trace-is-evidence-not-a-fixture.md)):
  the provenance travels with the number.

### 3. Who decides a hardware walk is needed, and the non-emulatable seams

Yona, verbatim (G2 Q2, on hardware-walk necessity):

> its up to planning agents whether or not real hardware walks are
> needed. my instinct is that they should only be used when it is likely
> the emulator is inaccurate or if the work is _changing_ a real
> hardware edge, like flashing firmware.

The call belongs to **planning agents**. The trigger list, ratified from
M6's G2 answer — a hardware walk is likely warranted when the work
touches:

- **byte-level serial-line interleaving** (the emulator does not model
  the physical UART's exact interleaving — see
  `docs/defects/2026-08-02-serial-line-interleaving.md`);
- **anything wall-clock-dependent** (emulated time is exact and
  deterministic; host wall-clock is not the board's, and no assertion in
  the emulated walk is ever about a duration);
- **a real hardware edge the work is *changing*** — flashing firmware
  is the worked example, and Chromium's own USB stack (device-loss
  reporting, Brave's grant revocation, the real chooser and its
  permission prompt) is the shim's named residue, hardware-only by
  design (ADR 2026-09-09 rule 3).

Everything above those seams — the product's own device logic — is what
the emulator walk covers, and where it is trusted.

## Two director-proposed riders (NOT yet Yona's — for the ship gate)

These two are the **director's proposals**, offered for Yona to keep or
strike when he takes the ship gate on this plan. They are not his words
and must not be read as ratified doctrine until he says so.

- **Rider A (director-proposed) — parity-fix-first has one escape, and it
  is not silent.** Rule 1 makes fixing the emulator gap the default
  before the underlying issue. When a hardware defect is too urgent to
  wait for emulator work, the fix may go first — but the emulator gap
  becomes a **mandatory filed defect at that same moment**, not an
  optional follow-up. The rule stays "fix parity first" precisely because
  the only way out is to write the gap down where the next walk will see
  it.

- **Rider B (director-proposed) — the fidelity-defect registry *is* the
  "reason to believe the emulator is wrong".** Rule 2 trusts an emulated
  measurement "unless we have a reason to believe the emulator is
  wrong." Make that concrete: the `docs/defects/` entries that record
  emulator/silicon divergence (e.g.
  `2026-09-10-the-emulated-c6-builds-a-graphics-stage-40x-slower-than-silicon.md`)
  are that register. A measurement is quotable when **no open fidelity
  defect covers its class**; if one does, the measurement is suspect for
  that class and says so. Quoted measurements carry the `lp-emu` git hash
  inline (extending M6's trace-provenance practice from fixtures to
  prose).

## Consequences

- A device walk for routine firmware work is a headless-browser recipe
  an agent runs, not a desk sitting scheduled around Yona.
- Every emulated number in a report or PR is expected to name `lp-emu`'s
  commit; a bare "measured X" without that provenance is now a smell.
- Planning agents own the hardware-walk decision and have a written
  trigger list to apply it against, so "do we need the board?" has a
  default answer (no) and a short list of exceptions.
- Parity is a first-class deliverable of a hardware walk: a divergence is
  a filed fidelity defect, and (under rider A, if kept) an emulator fix
  that lands before or alongside the product fix.

## Alternatives considered

- **Leave it as folklore in AGENTS.md.** Rejected: it is a rule about
  what evidence counts and who decides a walk, it reaches plan three
  (classic ESP32) and every future device plan, and Yona asked for it
  written down. That is an ADR, not a note.
- **Make the emulator walk *mandatory* and hardware *forbidden* for
  routine work.** Rejected: it over-reaches Yona's words. He kept the
  planning agent's judgement ("its up to planning agents") and named real
  seams where hardware is still the oracle. The norm is "emulator-first",
  not "emulator-only".
