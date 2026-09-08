# ADR: Always a device — a project declares a target, and something that acts as it runs the project

- **Status:** Accepted
- **Date:** 2026-09-07
- **Deciders:** Photomancer
- **Plan:** lp2025/2026-09-07-0118-studio-emulated-boards (P3, PR #586)
- **Supersedes:** the D22 section of
  `2026-07-15-device-session-model.md` — "the sim is not a device" is no
  longer true, and the type system no longer says it is
- **Amends:** `2026-07-24-runtime-pool.md` (one payload, capacity one),
  `2026-08-03-studio-runs-n-device-sessions.md` (the tab's one session is
  a device lens, whatever backs it),
  `2026-08-19-single-session-web-and-session-control.md` (the header
  control has one kind), `2026-09-01-editor-lens-borrows-the-device-wire.md`
  (the lens borrows a sim's wire the same way),
  `2026-08-08-project-url-identity-and-sharing.md` (the URL is the
  project for a sim lens; P4 adds the `?on=` hint),
  `2026-08-06-honest-device-preview.md` (one card-feed lane, the
  roster's), `2026-08-05-project-target-metadata.md` (`target` stops
  being advisory: it decides what runs),
  `2026-08-04-device-identity-anchored-in-silicon.md` (a Studio-minted
  locally-administered MAC is anchored by the same rule),
  `2026-09-03-device-card-fixed-height-and-disconnect-disappears.md` (the
  height table gains a sim-only row) — the reciprocal notes land in P6
- **Superseded by:** None

## Context

Studio had two kinds of runtime and one of them was a second-class
citizen by construction.

The **simulator** was a `LinkProvider` session with no link, no record and
no card of the device kind: `RuntimePayload::Sim(SimAttachment)` beside
`RuntimePayload::Device(DeviceLensAttachment)`, `RuntimeKind` to tell them
apart, `SimLoadedProject` to remember what it ran, `sim_link.rs` to open
it, `sim_card.rs` plus a whole card anatomy (`sim_card_state`,
`sim_rich_object`, `card_tabs`, `CardUiState`) to draw it, a `StopSimulator`
verb, a crash-reboot ladder nothing else had, and a second card-feed lane
in the runtime pool. Roughly 23 branch sites across seven files existed to
answer one question — *is this the sim?* — and every new device capability
had to be built twice or skipped for the sim.

That cost was paid for a distinction that does not survive contact with
where the product is going. Three sittings on 2026-09-07 (`vision.md`)
settled a different model, and its first claim is the one this ADR
records:

> **There is no runtime that is not a device.**

An emulator arriving as a third backing makes the old shape untenable
rather than merely expensive: `emu` is exactly as much a device as `sim`
is, and a second special case beside the first is not a design.

## The terms (copied verbatim from the vision, D40)

| Term | Meaning |
|---|---|
| **target** | What a project declares it runs on, and what a device acts as: a **board** (a catalog id) or **desktop**. A kind, never an instance (D32). |
| **real** | Silicon on the desk, or a real desktop LightPlayer server on the network. |
| **emu** | The real firmware binary for the target's chip, running in `lp-emu` (boards only — there is no desktop emu). Exact; slower. |
| **sim** | The desktop firmware (`fw-browser` = fw-desktop running in the browser) wearing the target's manifest — any target. Fast; not exact. The desktop sim wears the desktop manifest; a board sim wears that board's. |
| **preview** | The sim engine used internally for gallery previews. Never a device (D45). |

emu and sim are both **devices**: records in the Devices tab that act as
their target (D42). The project is the thing in the URL; `?on=` says which
device (D43).

The bands on the card (D49): real has none; **Emu · \<board\>**;
**Sim · \<board\>**; **Sim · Desktop**.

## Decisions

### 1. One payload

`RuntimePayload` has one arm: `Device(DeviceLensAttachment)`. The editor is
a lens on a roster device, always. `RuntimeKind`, `SimAttachment`,
`SimLoadedProject`, `sim_link.rs` and the sim's server-attach path are
gone.

What the old kind fork was *actually* about survives as
`LinkTransport { Sim, Serial }`, read off the link's own endpoint
(`sim:<uid>`) and carried on the attachment. It is a fact about the WIRE,
not about the device, and it decides exactly three things: the passive
pull cadence (33 ms in-process vs 150 ms serial), the product-subscription
scope (every expanded node vs the focused one), and the visual probe
resolution. Each of those is a bandwidth argument, and bandwidth is a
property of a wire.

### 2. A sim is a device record, a `Link`, and an effect backend

Nothing else. `BrowserWorkerLink` turns a `fw-browser` worker's envelope
channel into `lpa_devices::Link`; `SimDeviceTransport` serves the sims
this tab has powered on; a `CompositeDeviceTransport` routes by the
endpoint prefix. `lpa-devices` gained no arm, no flag and no knowledge
that a sim exists — the whole point of the aligned precedent,
`2026-08-05-device-first-creation.md` D2, *capabilities, not kind*.

Verbs mean what they honestly can: a flash writes nothing and restarts the
runtime; an erase clears memory by restarting; a manifest write re-dresses
the next boot — the same "effective next boot" a `/hardware.json` write
has on silicon. A push is the real `lpa-client` conversation, unchanged.
No sim effect reports a probed MAC or a chip name, because nothing probed
one.

### 3. Identity by the silicon rule, with a minted MAC

A sim's identity is a Studio-minted locally-administered MAC (`02:…`) run
through the same `HardwareId::from_base_mac` derivation silicon uses. The
registry key is the derived uid, exactly as for a board. The MAC is
minted once, at record creation, and stored in a
`/device-sims/<uid>.json` sidecar (v1: `kind`, `target`, `baseMac`,
`createdAt`) — the sole "this device is a sim" fact, and the reason a
library full of boards reads nothing new.

No sim flag on the wire (Q2). The hello reports `board_id` = the worn
manifest's id and `HardwareIdentity{base_mac}`, which is what real
firmware reports; sim-ness is a Studio record fact.

### 4. The band, and only the band

A sim card is the device card. The one mark that it is not silicon is a
24 px row under the identity rows, in the bound family:

```text
▶ Sim · Desktop · in this tab · GPU
```

The target it acts as, where the runtime lives, and the shader tier the
worker **granted** — never the tier that was requested, and dropped
entirely until a boot has answered. No title prefix, no tinted edge, no
second glyph, no violet anywhere else (D38). Real cards have no band at
all, which is why a real card's height is unchanged.

The band is joined at the app view (`DeviceRosterView::runtime_bands`),
like the card's feed, for the same reason: it is a fact about the runtime
behind a device, and the model must not learn to distinguish them.

### 5. One device per tab

Pool capacity is one, kind-agnostically (`SESSION_CAPACITY`). Opening a
project resolves a device, powers it on and lands the lens on it; if
another sim was on, it powers off silently (D37) and its record stays
(D46). If the lens was on another device, it closes first and that device
keeps running.

### 6. Opening a project never touches silicon

Opening resolves a SIM of the project's target — the one that last ran
this project, else an idle one of that target, else a freshly minted
record (D33). Putting a project on a real board is that board's own card
verb. Until the picker lands (P5), a board-target project mints its sim
silently on open, which is what D33 asks for.

Power on and power off are `Action::Connect` / `Action::Disconnect` at the
fold (Q15) — the same gestures a board's card raises. The UI labels them
**Power on** / **Power off** for sim-backed devices; the model has one
vocabulary.

The power-on **hello is held, not awaited**: the fold that produces it
runs on the actor's own queue, so an open that awaited it inside its
dispatch would wait for work that cannot run until it returns. The address
is held as `pending_device_lens` and the refresh tick lands the lens the
moment the device says hello — the same hold a `/device/<uid>` reload has
used since round 2.

### 7. The URL is the project

A lens on a sim emits the project route; a lens on silicon emits
`/device/<uid>`. P4 replaces both with the `?on=` grammar
(`/p/<slug>-prj<uid>?on=emu|sim|mac:|ws:`), at which point `/device/<uid>`
becomes a resolver.

### 8. The docs sim is anonymous

A docs page's leased controller powers on a sim whose boot options carry
**no identity**. The hello reports no MAC, `registry_key()` is `None`, and
`settle_device_records` never writes a row: reading the docs must not
leave a device behind in the reader's library (Q4). Previews are not
devices at all (D45) — they use the sim engine internally and have no
record, no card and no link.

### 9. Previews are not devices

Restated because it is the boundary that keeps (2) honest: a gallery
preview runs the same engine and is still not a device. What makes a
device is a record plus a link plus a card, and a preview has none of the
three.

## Consequences

- **D22 is retired.** The type system no longer encodes "the sim is not a
  device", because the claim is no longer true.
- **The reboot ladder is gone.** A crashed worker is `LinkEvent::Error`
  followed by `Closed`; the fold's existing not-responding face and the
  lens's dead-wire backstop cover it, and the card offers Power on rather
  than restarting behind the user's back. D22's "fake arm" (the guarded
  auto-reboot) was the last place Studio pretended a sim could not simply
  be a device that stopped answering.
- **The runtime pool's second card-feed lane is gone.** A sim is fed
  through `device_feeds` like every card — including the `Lens` treatment
  (last frame, dimmed, "the editor has the wire") while the editor holds
  its wire. A sim under the lens now shows a paused feed where it used to
  show a live one; that is the device behaviour, and it is honest.
- **A home open deploys into the dir the DEVICE says it runs from.**
  `attach_lens` asks every lens for its storage id; the sim's demo-slot
  assumption was its last special case.
- **Detaching the lens closes it.** The device keeps running, the session
  does not — for a sim exactly as for silicon. "Quiesce and keep the sim
  session running detached" has no meaning once the runtime outlives the
  session.
- **Every tab shutdown powers its sims off.** A running sim is a worker,
  and a worker nobody terminates outlives the page that started it; the
  actor does this on `Shutdown`, in one place, so no path leaks one.
- **The height table gains a row for sim cards only** (644 → 668 px). Real
  cards are byte-identical, which the story baselines pin.
- **The D28 "Running in simulator" line on a package card lost its
  source** and was removed rather than faked: it read the runtime pool's
  own record of what the sim ran, and the device pairing is the registry
  association a card Push banks, which a lens open does not write. Flagged
  as a follow-up.

## Alternatives considered

**A third payload kind.** Keep `RuntimePayload::Sim`, add
`RuntimePayload::Emu`. Rejected at the third sitting: it multiplies the 23
branch sites by the number of backings, and every device capability would
have to be written once per kind. The whole cost of the old shape was the
fork, and adding a third arm is paying it twice.

**A costume on Desktop (D34).** Let one Desktop sim *claim* to be a board
— an advisory `board_id` on the session, which is what
`sim_board_id` actually was — rather than boot wearing that board's
manifest. Rejected: a costume cannot refuse an output the board does not
have, so the one thing a board sim is FOR (finding out whether your
project fits the hardware) is exactly what it cannot do. Superseded by the
third sitting's ruling that a device acts as its target.

**A `is_sim` flag in `lpa-devices`.** Rejected on the anti-fifth-state-
machine rule and on D2's precedent: the model folds evidence, and "which
kind of thing is behind this link" is not evidence it needs. Everything a
sim does differently is either a transport fact (the endpoint says it) or
a record fact (the sidecar says it).

## Follow-ups

- The `?on=` grammar, the `/device/<uid>` resolver and the mismatch page
  (P4 of this plan).
- The Devices-page picker, the Hardware settings row and the new-project
  target default (P5).
- The vocabulary sweep, the glossary terms block and the reciprocal
  amendment notes in the nine ADRs above (P6).
- The package card's runtime-presence line: restore it from a pairing the
  fold actually banks, together with the D24 connected line and the "Live
  in 2 places" aggregate that retired with the old device system.
- The device trace's clipboard reader, which retired with the sim card's
  Console tab; the device card's Terminal zone is where it would live.
- The emu backing behind the same card (mode A) and the `navigator.serial`
  polyfill (mode B) — the emulator roadmap, unchanged by this ADR.
