# ADR: Device-first creation

- **Status:** Draft — the machine landed in P11; finalize in P06 when the UI
  lands and the G2 hardware walk has run.
- **Date:** 2026-08-05
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

Starting LightPlayer has meant starting with a *project*: open a package,
and the studio pushes it to a simulator nobody asked for
(`open_from_home_inner`'s hardwired push-to-sim). A board plugged into the
machine was a second-class thing you set up afterwards, through a form on a
device card.

The gallery product vision inverts that: **the device is the thing you
start with**, and the first project exists because a board needs something
to run. The flow is Connect → Flash → Provision → Device home, with a
simulator variant that must reach the same destination without hardware.

That put three questions on the table at once, and they are decided
together because separating them is what produced the old shape.

## Decision

### 1 · The flow is a pure reducer, not a chain of ops

`SetupFlow` (`lp-app/lpa-studio-core/src/app/setup_flow/`) is
`(State, Event) → (State, Vec<Command>)`: no I/O, no async, no UI types, no
clock. The UI renders `State`; a thin executor turns each `Command` into an
op the studio already runs.

The alternative — the shape the device card's setup form has today — is a
flow that IS its op sequence: each step awaits the next, and the states
exist only as intermediate variables inside async functions. That flow
cannot be tested without a device, so its failure edges are the ones nobody
exercises, which is precisely where flashing goes wrong.

Making the machine pure means every transition, including every failure
edge, is a table-driven unit test with zero I/O; the golden command traces
of the five paths (hardware happy path, sim, adopt, WLED wipe,
flash-fail-retry) are asserted as sequences. It also gives the M9 wasm fake
something to replay against.

**Cost accepted:** the flow's steps are now spread over a reducer, an
executor, and (in P06) a renderer, rather than reading top to bottom in one
function. The transition table in `docs/design/device-setup-flow.md` is the
compensating artifact, and the doc-and-code-in-one-commit rule is stated in
both places.

### 2 · The hardware/simulator boundary is capabilities, not kind

`SetupTarget` answers three questions — `needs_connect`, `needs_flash`,
`can_rename` — and the machine branches on nothing else. There is no
`is_sim`, and the flow-spec's separate `SIM_BOARD_PICK` state was merged
into `BOARD_PICK` entered with no probe evidence, because two states doing
the same job is the same branch wearing a disguise.

The payoff is that the simulator path is not a second implementation to
keep in sync: it is the same states, the same reducer arms, and the same
tests, minus what the target cannot do. And a test double that answers yes
to all three drives the FULL hardware path — flash included — with no
hardware attached.

### 3 · Board state is a verdict enum with an asymmetric bias

One probe pass produces one `BoardVerdict`: `LightPlayer` | `Wled` |
`Blank` | `Unresponsive`, each carrying `known: Option<RegisteredDevice>`
from a registry lookup keyed by the probed MAC's derived uid. It replaces
the implicit "did hello answer" checks that were scattered across this
seam.

The classification is deliberately biased. Under-claiming is cheap: a WLED
board read as `Blank` only means the wipe offer is skipped, and the flash
confirmation still guards the data. Over-claiming `LightPlayer` is not
recoverable by any later guard — it routes a stranger's board into the
adopt branch and offers to make it ours. So a proto-matching wire hello is
the **only** evidence that yields `LightPlayer`, and WLED detection ships
conservative (a banner match; ambiguous Improv traffic alone is not
enough).

### 4 · Naming is derived at provision, and the registry is the write

The device name is derived (remembered name > `<board label> · <Mon D>` >
collision suffix), editable, and written to the registry under the probed
`HardwareId` at PROVISION. There is no stamp step and no re-stamp dance:
identity is anchored in silicon and survives an erase
(`2026-08-04-device-identity-anchored-in-silicon.md`). The project keeps
the library's own dated-slug convention — this flow re-implements neither.

## Consequences

- The setup flow can be exercised end to end in unit tests, hardware or
  not; the hardware walk becomes the oracle for feel and real serial rather
  than for logic.
- `docs/design/device-setup-flow.md` and the reducer are one artifact. A
  transition change that lands in only one of them is a defect, and the
  doc's §7 records every place the implementation and the ratified
  flow-spec differ.
- The flash implementation and the card-owned op flows are untouched: they
  encode the espflash / S3-pty / #292 lessons and the flow composes them
  through `SetupDispatch`.
- WLED migration is out: the only WLED path is wipe-and-flash, and the copy
  says so.
- Not yet decided (P06): what a failed generate or push leaves behind on a
  board that is already flashed and registered. The machine has no
  PROVISION failure edge, which is honest about the gap rather than
  guessing at it.

## Alternatives Considered

- **Keep the async op chain and add tests around it.** Rejected: the
  failure edges are only reachable with a misbehaving device, which is the
  case that matters and the one no test can arrange.
- **A separate simulator flow.** Rejected: two flows drift, and the sim is
  supposed to be the *same* destination, not a preview of it.
- **A boolean `is_lightplayer` on the probe result.** Rejected: it makes
  the WLED and blank cases indistinguishable, and it puts the dangerous
  claim (this board is ours) on the cheap side of the branch.

## Follow-ups

- P06: the UI, the PROVISION failure edge, and finalizing this ADR.
- G2 hardware walk after P06.
- Verify WLED detection against a real WLED board; widen the markers only
  with evidence, never toward `LightPlayer`.
