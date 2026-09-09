# Virtual air: what an emulated radio may claim

- **Status**: accepted
- **Date**: 2026-09-08
- **Deciders**: the C6 emulator rounding-out plan (M4), Yona at G2
- **Supersedes / amends**: nothing. It is **new**, and the next section says why.
- **Related**:
  `docs/adr/2026-09-06-esp-soc-emulator-architecture.md` §"Honest peripherals:
  strict bus, `modeled` grades, and no invented answers" (cross-referenced from
  there, as a pointer);
  `docs/adr/2026-07-29-license-provenance-discipline.md`;
  `lp-emu/esp/lp-emu-esp32c6/README.md` ("The air, and the lockstep pair", and
  its honesty section);
  `docs/debt/emu-c6-air-delivers-every-other-frame.md`;
  `docs/debt/c6-scan-truncation-accepted.md`

## Why this is a new ADR and not an amendment

The architecture ADR's honesty statement governs **registers that answer**.
Its whole subject is a bus: an accept block remembers what was written and
gives it back, a register that nothing has measured is graded `modeled`, and
no block ever invents a value it was not given. Every rule in it is about
*responding*.

An air is a different kind of claim. It does not answer a read — it
**originates an event** the hardware would have originated, on its own
schedule, into a guest that did not ask. And it originates it on a completion
path **nobody has observed on silicon**: M4 P0 established that nothing on
this machine had ever seen a WiFi transmission complete, and M4 U1 made one
happen by finding what the blob's own interrupt handler responds to, not by
watching a chip do it.

Folding "we raise an interrupt the hardware would have raised" into a section
about register grades would blur the one distinction that section exists to
draw — between a value we were given and a value we made up. So the register
policy stays where it is, this policy sits beside it, and the architecture ADR
gains a single cross-reference line pointing here. That line is a pointer, not
an amendment: nothing in the register policy changes.

## The claim, in one paragraph

**The air is byte delivery on a perfect medium, and it is `modeled`.** A frame
the blob hands the MAC is read verbatim out of guest RAM at the descriptor the
blob itself programmed, and after a stated latency it is written into every
other machine's own RX descriptor ring behind an `rx_ctrl` header built from a
public layout, and the RX interrupt is raised; the sender's transmission is
then completed so its `send` returns. Nothing else about a radio is modelled —
no PHY, no channel occupancy, no collisions, no retries, no range, no rate, no
RSSI, no encryption and no timing windows — and the two events the air
**originates** rather than reproduces, the TX completion and the RX interrupt,
are raised on a path no silicon capture has ever shown us. Every bit of both
was chosen inside a constraint the guest itself imposed, which is why the
grade is `modeled` and not lower; none of it was measured against a board,
which is why the grade is `modeled` and not higher.

## The policy

1. **Byte delivery only.** An air may move bytes between machines. It may not
   model anything about the medium those bytes crossed. Every omission is
   listed **by name** in the machine's README, and a new omission is a README
   change, not a footnote.
2. **An originated event says so in the same breath.** Where an air raises an
   interrupt or completes an operation the hardware would have, the README and
   the code comment must say *that it is originated*, and must separate what
   the guest's own behaviour decided from what we chose. "The guest showed us
   which bit its handler responds to" and "we chose which slot completes" are
   different sentences and must be written as different sentences.
3. **A grade still moves only with a transcript.** This is the register
   policy's rule, unchanged, and it applies here without exception:
   byte-equality with silicon, agreement across configurations, and a
   guest-observed derivation are **evidence, weighed where a reader can see
   it**, never a promotion. As of this ADR the air has **no silicon twin at
   all** — `d1-desk-batch.md` step 3 is owed — so every claim it makes is
   `modeled`.
4. **A stated constant is stated, not derived.** The air's latency is one
   number in one place (`lockstep::DEFAULT_LATENCY_US`, 672 µs), and the
   arithmetic it was chosen from is written beside it. It does not vary with
   frame length, because varying it would be a PHY model wearing a disguise.
   It is never zero: a guest must not be able to see its own frame complete in
   the store that armed it.
5. **Determinism is the gate's form.** Two-machine gates run in lockstep, in
   one process and one thread, and are byte-identical across runs. A socket
   air is auditable only and may never be the form a gate takes — the same
   split the machine already draws between scripted input and live sockets.
6. **The air is off unless asked for.** A machine that has not joined an air
   is byte-for-byte the machine that came before it. An off switch that costs
   nothing, not a feature that is always half-on.
7. **Nothing is read out of a blob.** The `rx_ctrl` layout is derived from the
   public bit layout in the Apache-2.0 `esp-wifi-sys-esp32c6` 0.2.0 bindings,
   cited in a provenance header per the licence-provenance ADR and never
   copied; every other reading here comes from a register value, a symbol
   name, a call stack or a RAM dump. **No blob was disassembled**, and that is
   a constraint on the method, not a description of what happened to be
   convenient.

## What would have to be true for this to change

**A silicon capture of the completion mechanism.** Two boards on a bench
running the same payload, one transcript each, replayed against the emulated
pair on every non-timing field. If that capture agrees, the case for promoting
the air's claims above `modeled` can be made — and it is still a case to be
made and ruled on, not an automatic promotion. If it disagrees, the
disagreement is the finding and it is pinned rather than tuned away.

Until that capture exists, "ESP-NOW is modelled" is a sentence this repository
should be careful with, and the README's own wording is the place that
carefulness has to live.

## The consequence nobody should have to rediscover

An air that delivers is not the same as an air whose frames reach an
application. M4 P3's two-machine payload — the first thing on this machine to
send *and* receive repeatedly — found the receiving guest surfacing every
other frame written into its ring, with the air's own counters saying every
frame was delivered. The three claims "the air carried it", "the ring took it"
and "the application saw it" are **three different claims**, and an air that
reports only the first two will read as working long after it has stopped
being. `docs/debt/emu-c6-air-delivers-every-other-frame.md` is that finding;
the payload's `gap` field is how it reaches a replay.
