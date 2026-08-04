# ADR: A device reports its own chip identity

- **Status:** Accepted
- **Date:** 2026-08-03
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

Two complaints met in the same conversation.

The standing one: **there is not enough hardware information on the device
card.** Before a hello, the Technical tab could say only "transport USB"
(improved at the 2026-08-03 gate-1 sitting with chip + port label); after
one, it reported build provenance and capability gaps but nothing about
the silicon itself.

The sharper one came out of the multi-board work. Surveying what identity
signals Studio actually has produced an uncomfortable table:

| Signal | First available | Identifies |
|---|---|---|
| VID:PID | pre-connect | a CLASS — `303a:1001` is every native-USB Espressif |
| USB serial number | never | Web Serial does not expose it |
| grant / endpoint id | pre-connect | a GRANT, not a board |
| `detected_chip` | pre-hello | a CLASS (`esp32s3`) |
| `dev_…` uid | post-hello | an INSTANCE — but ours, and erasable |

**The only instance identity was one we invented.** `dev_…` is minted by
Studio, written to `/.lp/device.json` on the device filesystem, and read
back through the hello — so `ResetToBlank` (`EraseDeviceFlash`) destroys
it and the board becomes a stranger. Nothing the board knows about itself
ever reached Studio.

Meanwhile the chip has carried a unique, permanent, factory-burned MAC the
whole time, and the firmware was already reading it — `fw-esp32c6`'s
ESP-NOW driver derives its radio device id from
`efuse::interface_mac_address(Station)`. It was simply never put on the
wire.

## Decision

**The device reports its own chip identity in the hello.** `HardwareFacts`
gains `base_mac`, `chip_revision`, and `eui64`; embedders supply them
through `LpServer::set_hardware_identity`, mirroring how build provenance
arrives through `set_hello_identity`.

**Report the BASE MAC, not a list of per-interface addresses.** Wi-Fi
Station *is* the base address; SoftAP and BLE are derived from it by a
published rule (set the local-admin bit; BLE additionally bumps the last
octet). Shipping the derivations would be redundant, and it would invite
drift from what the radios actually use — `esp_hal` exposes
`override_mac_address`, so a value we derived could be wrong outright.

**An interface reports its OWN address only when that interface is
genuinely wired** — the rule `HardwareFacts` already follows for `radio`
and `button`. Ethernet and BLE therefore report nothing today, because
neither driver exists. When they land, each reports what it actually
uses, from the device.

**802.15.4 gets its own field, because it is a different width.** The
EUI-64 is 64 bits — the 48-bit base MAC plus the chip's 16-bit `MAC_EXT`
efuse field — and only parts with an 802.15.4 radio have `MAC_EXT` at all
(the C6 does; the classic ESP32 does not). Widening `base_mac` would have
made the width a property of the format instead of a property of the
radio.

**The efuse read lives in each per-SOC crate, not in the shared one.**
`fw-esp32-common` must build under both toolchains and takes no `esp_hal`
dependency (ADR 2026-07-29-per-chip-fw-toolchains: *chip facts arrive by
injection*). It contributes only the wire formatting — lowercase colon
hex — which every chip must agree on.

## Consequences

- The device card's Technical tab shows `mac`, `eui-64` where the chip has
  one, and the silicon revision folded into the existing `chip` line
  (`esp32c6 · rev 0.2`) rather than spending a row on one fact.
- **A board now has an identity that survives an erase.** This is the
  durable part. It is not yet USED for identity reconciliation — the
  registry still keys on `dev_…` — but the fact is now on the wire, which
  is the prerequisite. Reconciling a re-provisioned board back to its
  registry entry by MAC is deliberate follow-up work, not a side effect of
  this change.
- It does **not** help a board that cannot boot. Everything here is behind
  the hello, so a board stuck rebooting reaches no layer where identity
  exists. That is why the boot-loop defect
  (`docs/defects/2026-08-03-boot-looping-board-reads-as-flicker.md`) is
  handled as an identity-free advisory instead of as a card.
- `HardwareFacts` derives `Default`. Every field's default is "nothing
  known", which is the honest report for an embedder that cannot answer,
  and it stops each added field from churning every test fixture — a cost
  paid the same day on `UiCardConnection`.
- **No `WIRE_PROTO_VERSION` bump.** The fields are additive and optional:
  they carry `#[serde(default)]`, and nothing sets `deny_unknown_fields`,
  so an old firmware's hello reads as `None` on new Studio and a new
  firmware's extra fields are ignored by old Studio. Every past bump was a
  breaking change, and a version difference means "assume nothing works" —
  bumping here would have marked every board on current firmware
  Incompatible in exchange for nothing.
- Embedders without efuse (`fw-host`, `fw-browser`, `lp-cli`, `fw-emu`)
  never call the setter and report `None`, exactly as they already do for
  build identity.

## Alternatives Considered

- **Report every interface MAC (STA/AP/BLE/ethernet).** Rejected: they are
  derivations of one value by a published rule, so the extra fields carry
  no information and can disagree with reality once `override_mac_address`
  is in play.
- **Derive the interface addresses host-side from the base.** Rejected for
  the same reason, more strongly: the host cannot know about an override.
- **Put the efuse read in `fw-esp32-common`.** Rejected — it would need
  `esp_hal`, which that crate's seam rules forbid. Attempted first, and
  the build failure was the ADR enforcing itself.
- **Widen `base_mac` to hold the EUI-64.** Rejected: the width belongs to
  the radio, not the identity, and the classic ESP32 has no `MAC_EXT` to
  widen with.
- **Use the MAC as the primary device identity now, replacing `dev_…`.**
  Deferred, not rejected. The `dev_` uid also carries a user-chosen name
  and a registry association; swapping the key is a migration, and this
  change is the prerequisite for considering it.
