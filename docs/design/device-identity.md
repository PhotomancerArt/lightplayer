# Device identity anchored in silicon

Status: PARTLY DELETED 2026-08-25 (M2 of the device-model rebuild); as-built
2026-08-04 (ADR: 2026-08-04-device-identity-anchored-in-silicon)

> ⚠️ **The connect-time RESOLUTION described here is deleted.** M2 of the
> device-model rebuild removed `identity_resolution.rs`,
> `places/device_identity.rs` and every flow that stamped or promoted a
> uid. What SURVIVES is the durable half — `HardwareId` (the canonical
> origin format) and `DeviceRegistry` (the record store and its on-disk
> format), untouched and still the store the rebuilt model reads. The
> rebuilt model's identity chain (endpoint → MAC → uid → name, with
> promotion and merge as journaled operations) supersedes §3-§6 here.

## §1 · Problem (the pre-2026-08-04 scheme)

A LightPlayer device's only instance identity used to be one Studio
invents: a random `dev` uid minted at provisioning
(`run_identity_stamp`), written to `/.lp/device.json` on the device
filesystem, and read back through the hello and a direct fs read at
connect. Consequences, all observed on the bench:

- **Erase amnesia.** `EraseDevice` destroys the file; the board
  reconnects as a stranger. The registry row, name, board choice, and
  push association all orphan.
- **Unstamped limbo.** A board has NO durable identity until
  mid-provision. Blank, WLED, and erased boards can never be
  recognized, so the setup flow cannot say "this is Porch sign".
- **The key flip wart.** An anonymous live card keys by session
  (`runtime-N`); the stamp flips its key to `dev…` mid-flow, orphaning
  `CardUiState` (`ui_device_card.rs:90` cascade).
- **Grant-heuristic continuity.** Replug continuity rides the Web
  Serial grant endpoint id (`migrate_card_op`) — a fact about the
  GRANT, not the board.
- **A needless write.** Provisioning must perform a stamp step — a
  wire write whose only purpose is to give the board a name tag we
  then struggle to keep attached.

Meanwhile every ESP chip carries a factory-burned, erase-proof,
globally unique base MAC in efuse, and since ADR 2026-08-03 the
firmware already reports it in the hello (`HardwareFacts.base_mac`) —
display-only. That ADR deliberately deferred using it as identity;
this design is that follow-up, timed so the setup-flow state machine
(gallery plan P11) is specified against the new model rather than
migrated to it later.

## §2 · Design overview

Two moves, one small and one deliberately tiny:

**I1 — `HardwareId`: the semantic identity source.** A transport-class
scoped enum naming where a unit's identity comes from:

```rust
/// Durable identity of a physical unit. Silicon when the transport
/// class has it; minted when it doesn't.
pub enum HardwareId {
    /// ESP-class silicon: the factory base MAC from efuse
    /// (HardwareFacts::base_mac / a download-mode ROM read).
    EspEfuse { mac: [u8; 6] },
    /// Host-class embedders (fw-host, lp-cli) and legacy/stamped
    /// devices: a random `dev` uid (today's scheme, demoted to
    /// fallback).
    Minted { uid: PrefixedUid },
}
```

**I2 — the registry key stays a `dev` uid; its ORIGIN changes.**
`HardwareId::device_uid()` maps to a `PrefixedUid`:

- `Minted { uid }` → `uid` (unchanged).
- `EspEfuse { mac }` → `PrefixedUid::mint(Device, bytes)` where
  `bytes` = 16 bytes: `[0u8; 9] ‖ [0x01] ‖ mac48`. Deterministic and
  injective; `mint` already takes caller-supplied bytes (no rng in
  `lpc-history`). The derived body renders zero-prefixed
  (`dev00000000xxxxxxxx`-ish), so origin is readable at a glance and
  collision with the random 80-bit mint space is ignorable.

This is the blast-radius decision (D1). `RegisteredDevice.uid`,
`DeviceAssociation.device`, the Connected/Pushed history events, the
card `identity_key()` cascade, `device_id_for_card_key`, and the wire
all keep the `dev` strings they already speak. Project history files
— durable, and they travel with the project — need no format change.
The alternative (keying everything on a new `HardwareId` string form)
re-types durable files for zero user-visible gain.

A deliberate corollary (D2): the derivation is a transparent
**embed**, not a salted hash. Today two studio installs agree on a
board's uid because the device carries it; deterministic derivation
preserves that agreement, a per-store salt would silently lose it.
The MAC is recoverable from the uid either way (48-bit space; a hash
brute-forces), so privacy is handled where it belongs — the export
boundary (§8), not the key.

## §3 · Identity acquisition rules

In precedence order, per session:

| Rule | Source | When | Yields |
|---|---|---|---|
| A1 | `hello.hardware.base_mac` | LightPlayer firmware answers | `EspEfuse` |
| A2 | download-mode ROM MAC read (esptool-js, same loader session as the chip check) | explicit setup flows only: flash preflight, wizard PROBING | `EspEfuse` |
| A3 | `/.lp/device.json` legacy read (wire fs read / hello `device_uid`) | no MAC available: host-class embedders, pre-2026-08-03 ESP firmware | `Minted` |
| A4 | none | anonymous | session-scoped only (`runtime-N`), exactly today's unstamped behavior |

Hard rule for A2: **never passively reset foreign firmware into the
bootloader.** A MAC read outside an explicit user-initiated setup/flash
flow is forbidden — WLED boards on the bus are guests. Pain neighbors
to respect in implementation: #292 (esptool-js DIE-name refusals —
normalization now lives in `lpa-link/src/provider/chip.rs`), CH340
reset quirks, S3-pty flashing (bench-side espflash, not the browser
path).

Host-class embedders (`fw-host`) keep the file convention as their
`Minted` store — a host filesystem has no efuse but also doesn't get
erased by a flash tool, so the file is honest there.

## §4 · Registry changes

`RegisteredDevice` (`places/device_registry.rs`):

```rust
pub struct RegisteredDevice {
    /// `dev…` — now DERIVED from silicon when the source is EspEfuse
    /// (I2); minted only for host-class/legacy rows.
    pub uid: String,
    /// The identity source, canonical string form
    /// ("efuse:aa:bb:cc:dd:ee:ff" | "minted"). Optional: legacy rows
    /// lack it until re-keyed at next sighting.
    pub hardware_id: Option<String>,
    /// Uids this device previously wore (the stamped uid a re-keyed
    /// row migrated from) — resolves old history events for display.
    pub previous_uids: Vec<String>,   // #[serde(default)]
    // name, transport, last_seen_at, association, board_id unchanged
}
```

Name, `board_id`, and association were already registry-resident;
they now become registry-**only** (§5). The registry stays the
user-facing naming truth (D34 rules unchanged: sightings never rename;
merge preserves association/board).

### Migration: lazy re-key at sighting

`/registry.json` is local (OPFS) — a one-time re-key is acceptable
(share-envelope "version + refuse" posture applies to things that
travel; this doesn't). But an offline re-key is *impossible*: old rows
hold random uids and we don't learn the MAC until the board shows up.
So migration is lazy, at connect:

1. Session resolves `HardwareId` per §3 → `derived = device_uid()`.
2. If the hello/file ALSO carries a stamped uid and the registry has a
   row under it but none under `derived`: **re-key** — move the row to
   `derived`, push the old uid onto `previous_uids`, set
   `hardware_id`.
3. If rows exist under BOTH (board was sighted by both schemes):
   merge into `derived` — registry name wins by D34, most-recent
   association wins, union `previous_uids` — and drop the old row.
4. Rows for devices never sighted again keep their old keys. Harmless;
   `forget` already exists for hygiene.

No registry file version bump: new fields are `#[serde(default)]`
additive, old rows parse as legacy (`hardware_id: None`).

## §5 · Provisioning: the stamp step is deleted

- The wizard/setup-form flow stops calling `run_identity_stamp`. The
  chosen name writes to the **registry** under the probed
  `HardwareId` — no on-device identity write, no re-pull for adoption
  re-classification (identity is known from probe time, so
  `PendingIdentity` classification disappears for ESP boards).
- `/.lp/device.json`: never written for ESP-class. Legacy READ paths
  stay (A3) for migration and host-class. `run_identity_stamp`
  survives, demoted to the host-class path (or inlined there).
- Optional name write-down to the device is a **projection, never
  truth** — the concrete channel is the networking branch's `net.json`
  hostname (PR #319), which derives from the registry name when that
  work lands. Nothing reads a device-side name back except as debug
  display.
- `DeviceContent::PendingIdentity` (adoption-waits-for-stamp) becomes
  reachable only in the A4 anonymous corner (old firmware + no file);
  the wizard never needs it.

## §6 · Consequences

- **Erase stops destroying identity.** `EraseDevice` wipes projects
  and (legacy) the file — but the MAC survives by construction. The
  D41 confirm-sheet copy changes to match ("erases its projects — the
  board stays remembered"). Re-flash lands on the same card, same
  name, same association history.
- **Forget must revoke the port grant, not just the row** (G2 walk,
  2026-08-05). Silicon-anchored identity is re-derivable by
  construction, so a registry-only forget is undone by the next page
  load: the Web Serial grant outlives the page, the app re-enumerates
  the granted port, auto-probes, re-derives the same `dev` uid, and
  the sighting write recreates the deleted row. `HomeOp::ForgetDevice`
  therefore disconnects the live session, revokes the grant through
  `LinkProvider::forget_endpoint` (`SerialPort.forget()`), and only
  then deletes the row. A grant is nameable only through a live
  endpoint — no pre-connect mapping from grant to board exists — so an
  OFFLINE device is registry-only and any grant it holds survives.
  That is harmless on its own: a grant with no row re-registers only
  if the user reconnects that board.
- **Blank/erased/WLED boards are recognizable** the moment a MAC is
  read (A2): the wizard can say "This board was **Porch sign** — it's
  currently blank / running WLED." Adopt verdicts enrich accordingly
  (§7).
- **The key flip narrows but doesn't vanish.** Identity still arrives
  after connect (at probe/hello, not at grant), so `runtime-N → dev_`
  still happens — but seconds in, before user interaction builds card
  state, and for known boards it lands on the remembered row.
  Adjacent cheap fix, in scope: migrate `CardUiState` across the flip
  the way `migrate_card_op` migrates ops.
- **Endpoint-grant continuity demotes to a pre-identity hint.**
  `migrate_card_op` stays (op flows are session-scoped and the replug
  case is real); it just stops being the board's identity.
- **Sim unchanged.** The sim is not a device (D22); `"runtime-sim"`
  reserved key stays; no `HardwareId` for it.
- **`identity_key()` cascade unchanged in shape** (`uid ?? session_key
  ?? name`) — uid simply exists earlier and more often.
- **Spoofing is an explicit non-goal.** A MAC identifies; it never
  authenticates (`esp_hal` has `override_mac_address`; anyone can
  claim any MAC). Trust-on-first-sight is the alpha posture;
  authentication is the separate trust-model work flagged in the BLE
  feasibility notes.
- **Clones (D5).** Two live boards with the same MAC: surface a
  warning card state; the second board stays session-scoped and is
  not remembered. No minted-fallback stamping for clones at alpha —
  machinery with one exotic user.
- **Old firmware (D6).** Pre-2026-08-03 hellos lack `base_mac` → A3
  resolves them via their stamped file, unchanged behavior.

## §7 · Gallery amendments

The amendments from this design were applied to the gallery plan
(device-setup-flow, P11, P06) on 2026-08-04. The setup-flow state
machine (gallery P11) consumes identity-at-probe via the probe result's
`hardware_id: Option<HardwareId>` field; registry write replaces the
stamp step; provision naming prefills from the registry for known
boards (amended F3); adoption verdicts enrich to carry `known:
Option<RegisteredDevice>` for recognition ("This board was Porch sign
— currently blank/WLED").

## §8 · Future work (explicitly out of scope)

- Share-envelope hygiene: strip or pseudonymize device refs
  (associations/history device uids embed MACs) at export. Envelope
  posture today is version+refuse, so nothing regresses now.
- Auth/trust model (BLE feasibility gap). Networked-transport
  identity rides free (any transport with a hello carries `base_mac`),
  but authenticating it is separate work.
- Multi-studio registry sync — uids now agree across stores by
  construction; syncing the rows themselves is its own feature.
- Removing the firmware's `device_uid` hello field / file read
  entirely (wire-compat cleanup, post-migration).
