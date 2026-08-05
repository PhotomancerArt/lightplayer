# ADR: Device identity anchors in silicon

- **Status:** Accepted
- **Date:** 2026-08-04
- **Deciders:** Photomancer
- **Supersedes:** None (extends 2026-08-03-device-reports-its-chip-identity)
- **Superseded by:** None

## Context

A device's instance identity is a `dev_` uid Studio mints at
provisioning and stamps into `/.lp/device.json` on the device
filesystem. Everything downstream keys on it: the registry row, the
push association, project-history Connected/Pushed events, the card's
`identity_key()`. The scheme has a structural flaw the 2026-08-03 ADR
named while shipping its prerequisite: **the identity is ours, and it
is erasable.** Flash-erase makes a board a stranger; a board has no
identity at all until mid-provision, so blank/erased/WLED boards can
never be recognized; the mid-flow arrival of the uid flips the card
key and orphans UI state; and replug continuity leans on a Web Serial
grant heuristic because nothing better existed pre-hello.

Since that ADR, the chip's factory base MAC — permanent, unique,
erase-proof — arrives in every hello (`HardwareFacts.base_mac`),
display-only. The same MAC is readable in download mode through the
esptool-js loader session the flash preflight already opens, i.e. in
every board state that matters to the setup flow. The gallery rework
is about to specify the setup-flow state machine (P11); its command
set differs under the two identity models, which is why this decision
is being made now rather than migrated to later.

## Decision

**A device's identity is its silicon.** A transport-class-scoped
`HardwareId` names the source: `EspEfuse(mac)` for ESP-class hardware,
`Minted(dev_uid)` as the fallback for host-class embedders (no efuse)
and legacy sightings.

**The registry key remains a `dev_` uid; its origin changes.**
`EspEfuse` maps to a `PrefixedUid` **deterministically**:
`mint(Device, [0u8;9] ‖ [0x01] ‖ mac48)` — injective, zero-prefixed so
derived uids are visually distinct from random-minted ones.
`PrefixedUid::mint` already takes caller-supplied bytes; determinism
is a supported use, not a hack. Registry rows, associations, history
events, card keys, and the wire keep the `dev_` strings they speak
today — the durable per-project history format does not change.

**The derivation is a transparent embed, not a salted hash.** The
stamped file used to give independent studio installs uid agreement on
the same board; deterministic derivation preserves that, a per-store
salt would silently lose it. The MAC is recoverable from the uid under
any unsalted scheme (48-bit space), so privacy is an export-boundary
concern (share envelopes strip or pseudonymize device refs — future
work), not a key-derivation concern.

**Identity acquisition, in precedence order:** (1) the hello's
`base_mac`; (2) a download-mode ROM read **inside explicit setup flows
only** — flash preflight and the wizard's probe; never a passive reset
of foreign firmware; (3) the legacy `/.lp/device.json` read, which
remains for host-class embedders and pre-2026-08-03 firmware; (4)
nothing — the board stays session-scoped, exactly today's unstamped
behavior. A MAC that reads as all-zeroes or all-ones is a FAILED efuse
read, not an address: it is rejected at both readers, because
accepting it would hand every failed board one shared identity.

**The provisioning stamp step is deleted.** The chosen name writes to
the registry under the probed `HardwareId`. Name, board choice, and
association are registry-only data; any device-side copy (the
networking branch's `net.json` hostname) is a projection written from
the registry, never read back as truth. `/.lp/device.json` is no
longer written for ESP-class devices.

**Migration is a lazy re-key at sighting.** The registry is local, but
old rows can't be re-keyed offline — the MAC isn't known until the
board appears. On a sighting that yields both a MAC and a stamped uid,
the stamped row moves to the derived uid, recording the old uid in
`previous_uids` so old history events still resolve for display. Rows
whose boards never return keep their old keys; `forget` covers them.

**A sighting is not a registration.** A MAC-identified stranger has a
uid from its first hello, but the registry only remembers boards we
were told about: one already carrying a legacy stamp, one whose
content is adopted, or one the user names. Being seen never creates a
row.

## Consequences

- **Erase no longer destroys identity.** Re-flash, erase, and WLED
  takeover all leave the board recognizable; the erase confirm copy
  changes from "the board becomes a stranger" to "erases its projects".
- **The setup wizard can recognize boards in any state**: probe
  verdicts (LightPlayer/Wled/Blank/Unresponsive) gain a registry
  lookup keyed by MAC — "This board was Porch sign — currently
  running WLED."
- The setup-flow reducer (gallery P11) loses the `stamp-name` command
  and gains identity-at-probe; the name field prefills from the
  registry for known boards.
- **"Has a uid" stops meaning "is named."** Every gate that used to
  read `identity.is_none()` as "this board needs a name" now reads the
  NAME: the Needs-a-name card, the push refusal, the post-flash setup
  name. Gently insisting on a name is a product behavior, and it must
  not be repealed by the uid arriving earlier.
- The `runtime-N → dev_` card-key flip narrows (identity arrives at
  probe, before interaction builds card state) but is not eliminated;
  `CardUiState` migrates across the flip as ops already do.
- The Web Serial grant heuristic demotes to a pre-identity hint; op
  migration across replug stays session-mechanics.
- **A MAC identifies; it never authenticates.** `override_mac_address`
  exists; spoofing and clone hardware are explicitly out of scope
  (trust-on-first-sight at alpha; the BLE-feasibility trust gap is
  separate work). Two live boards claiming one MAC surface a warning;
  the second stays unremembered — and, concretely, stays anonymous, so
  two cards can never share an `identity_key()` (a keyed-list
  duplicate panics Dioxus).
- Host-class embedders keep the minted-uid file — an honest store on a
  filesystem no flash tool erases.
- Download-mode reads inherit the probe neighborhood's known pain:
  #292 chip-name normalization, CH340 reset quirks. The read rides
  loader sessions that already exist; no new reset paths.
- The sim is untouched (not a device, D22).

## Alternatives Considered

- **Keep the stamped uid (status quo).** Rejected: every observed
  identity defect — erase amnesia, unstamped limbo, the key flip, the
  grant heuristic — is downstream of identity being invented late and
  stored in erasable media.
- **Key the registry on a new `HardwareId` string, drop `dev_` uids.**
  Rejected: re-types the durable project-history event format and
  every uid-speaking surface for zero user-visible gain; the derived
  uid gets the same semantics into the existing type.
- **Salted-hash derivation (per-store).** Rejected: silently loses
  cross-install uid agreement that the stamped file provided, and buys
  no real privacy (48-bit brute force). Export-boundary hygiene is the
  honest fix.
- **Stamp the MAC into `/.lp/device.json` instead.** Rejected: keeps
  the write, keeps the erasable copy, adds a second source of truth
  that can contradict efuse.
- **Passive download-mode identity probe on connect.** Rejected
  outright: resetting foreign firmware (a running WLED install) out
  from under its owner to read a MAC is exactly the behavior the probe
  rules forbid.

## Follow-ups

- The provisioning flow's `run_identity_stamp` call is still in place;
  the connect path already treats its output as legacy evidence, so a
  stamped board re-keys to its derived uid on the next pull with its
  name intact. Removing the call (and demoting the function to the
  host-class path) is the provisioning phase's work.
- Share-envelope hygiene: associations and history events embed device
  uids, and a derived uid embeds a MAC. Strip or pseudonymize device
  refs at the export boundary.
- Multi-studio registry sync: uids now agree across installs by
  construction; syncing the rows themselves is its own feature.
- Removing the hello's `device_uid` field and the `/.lp/device.json`
  read entirely, once no fielded board still needs migrating.
