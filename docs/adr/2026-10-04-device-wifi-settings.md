# ADR: Wi‑Fi settings on the device — a write-only network file, set over USB and Bluetooth

- **Status:** Proposed (the ship gate decides; WQ2–WQ4 accepted by Yona,
  2026-10-04: "yes, accept all three")
- **Date:** 2026-10-04
- **Deciders:** Yona (WQ2–WQ4), the director (WQ1, WQ5–WQ8, Q1–Q13)
- **Refines:** `docs/adr/2026-09-23-ble-access-model.md` ("WiFi reuses the
  model"), `docs/adr/2026-10-02-board-ids-and-typed-offer-parameters.md`
- **Evidence:** planning dir `lp2025/2026-10-04-0808-wifi-settings/`
  (`plan.md`, `notes.md` WQ1–WQ8 and Q1–Q13); the Wi‑Fi roadmap
  `lp2025/2026-10-01-1832-wifi-control` (M5; decisions D1–D11); PR #972.

## Context

The Wi‑Fi roadmap's M6 teaches the C6 to join a network. Before it can, a
board needs somewhere to keep the credentials and a way to be told them,
and it must never hand the password back. The access model already had the
shape: device secrets in `/.lp/`, a tier per request, a write-only rule on
the fs path, trust as a property of the link. M5 adds the network to it and
stops there: nothing joins, nothing on the radio changes, and every image
says so.

## Decision

### The file: `/.lp/network.json`, its own `version: 1`

```json
{"version":1,"wifi":{"ssid":"lp-walk-net","password":"…","enabled":true},"cloudRelay":true}
```

`lpc_access::NetworkFile` (`lp-core/lpc-access/src/network_file.rs`), schema
`schemas/device-network.schema.json` (generated, `just schema-check`). A
sibling of the device store `/.lp/access.json` inside `lpfs`, so the backup
archive and the C6 layout migration carry it like every other file.
`wifi` is absent when no network is saved. **Missing** = no network, relay
allowed; **damaged** = no network, logged (never its bytes), not rewritten
until the next change. Validation follows 802.11/WPA2 and is the board's:
SSID 1–32 bytes of UTF-8; password `""` (open), 8–63 printable ASCII, or
64 hex digits. No error or `Debug` prints a password.

Bump rule: a serde change to `NetworkFile` or `WifiNetwork` is a format
change even with no field added — bump `VERSION`, keep a reader for every
older version, pin the old bytes in a test, regenerate the schema. The
device reads old and writes current; `lpa-upgrade` (projects) is not
involved and the file is outside `PROJECT_FORMAT_VERSION`.

Not inside `access.json` (WQ1's alternative): the access store has its own
version history and readers, and a network is not access — two small files
each with one job are easier to evolve than one file with two.

### Write-only on every link

One predicate, `lpc_access::is_write_only_file_path`, matches
`.lp/access.json` and `.lp/network.json` (any spelling: `.`/`..`, empty
components, ASCII case) and replaced `is_access_file_path` at every call
site: `FsRequest::Read` is refused on every link at every tier including
trusted USB, changes-since skips it, a package hash that would cover it is
refused, a loaded project cannot read it as a resource, and Studio's zip
export skips it. A listing may name it; raw writes stay allowed at edit (a
restore writes `.lp/` files; a bad hand-written file reads as no network).
The server reads it only through its own base filesystem (`network_store`).

### Plaintext at rest, and why (WQ2)

The password is stored as typed in `lpfs`. A C6 has no secure element and
no flash encryption in this product, so there is no secret on the board to
hide a key in: whoever holds the board (the cable, a flash read) already
has everything on it — the access model's threat model. What the product
guarantees is that **no link reads it back**. Rejected: XOR/obfuscation with
the MAC (the MAC is readable by the same attacker; it buys only "not
visible in `strings`"); storing the WPA2 PMK derived in Studio (WPA3-SAE
needs the passphrase, and whether esp-radio accepts a raw 64-hex PSK is
unverified — M6 may revisit); ESP32 flash encryption (an irreversible eFuse
burn that breaks Studio's raw `lpfs` reads and the esptool-js update flow).

### Who: edit tier, every link that holds it (WQ4)

`ClientRequest::{NetworkStatus, NetworkSet, NetworkForget}` are all
`Required::Edit` in the exhaustive classifier; nothing at play. USB always
holds edit; Bluetooth at author; M6's keyed links when they arrive. A
fresh board is open at edit (`2026-10-02-two-passwords-open-by-default.md`),
so **anyone nearby can set a fresh board's Wi‑Fi** — the same exposure as
everything else on an open board. A board holding its files for the C6
layout change (`fs: legacy_held`, a RAM filesystem) refuses set and forget
("finish the update first") and answers status.

### Accepted limitation: the password crosses Bluetooth in the clear (WQ3)

BLE links are not sealed yet (`2026-10-01-network-link-security.md` lists
BLE `secured()` as later). A sniffer in range at the moment of provisioning
over Bluetooth captures the house Wi‑Fi password. Accepted for M5; sealing
the BLE link (`ble().secured()` on lp-link) is the follow-up that closes it.
USB is the trusted, physical link.

### Also holding the password (Q11, Q12)

- A session recording (`?record=`), a `?wire-capture=1` capture or an
  `LP_EMU_WIRE_TAP` tap taken while setting Wi‑Fi holds it in the raw
  transport bytes (USB is not encrypted). Redacting raw bytes would break
  what a capture is for; the structured request log names only `wifi.set`.
  Transports that echo raw request text into a log (fw-core's serial
  transport, the BLE line preview) withhold any line that names
  `networkSet` (`lpc_wire::may_carry_secret`).
- The device backup archive (`lp-cli hardware lpfs save`, Studio's migration
  backup) carries `network.json` in plaintext, as it carries the access
  keys: a restore must bring Wi‑Fi back. Treat an archive like the board.

### One relay switch: `cloudRelay`, default true (WQ5)

The relay is on by default (roadmap D9): `cloudRelay: true` lets
lightplayer.app reach the board through the cloud, `false` keeps it to its
own network. A missing field reads as on. The switch is named for what it
turns on, not for what it forbids (Yona's review of PR #972: "positive
options are better"), so Studio's toggle reads "Cloud relay", on by
default, with "Lets lightplayer.app reach this board through the cloud"
under it, and `lp-cli wifi set --cloud-relay on|off`. Studio shows the
switch now, with "applies once this firmware uses the relay" while the
firmware does not join.

### The status never carries the password (WQ6)

Every request answers `ServerMsgBody::NetworkStatus { wifi?: { ssid,
hasPassword, enabled }, cloudRelay, station }`. The SSID reads back (it is
broadcast anyway); the password never. A new SSID requires `password`
(`""` for open), so an old password is never offered to a new network.
`station` is `unsupported | off | joining | joined{ip,rssi} |
failed{reason}`: every M5 image says `unsupported` (an injectable probe on
`LpServer`, unset); M6 fills the rest. Relay state is M7's. Wire proto
**36** (the plan named 35; OTA M1 took it); `PACK_FORMAT_VERSION`
unchanged (the learned dictionary needs nothing for new variants).

### Secret offer parameters (WQ8)

Studio's Wi‑Fi verbs are offers at `devices/<board>/wifi/{set, enabled,
cloud-relay, forget}` (Forget Lasting). Offer `Text` parameters gain
`secret`: a renderer draws it as a password field; a press's stamp
(`OfferPress.args`, what the app agent hears) carries `•••`
(`SECRET_MARKER`) in its place while the binder sees the real value; the
op's `Debug` (the session recorder's `fmt_op`) redacts; the agent's
readout says "the user types it"; `act` refuses any value for a secret;
and an offer that takes a secret is always the user's card. The password
never reaches the model provider, and Studio never stores it — it lives in
the form until the press and in the op until the request leaves.

## Consequences

- One more device secret behind the same gate; the gate is now named for
  what it does (`is_write_only_file_path`), so a third secret file is one
  line in one predicate.
- Every firmware image grows by the handlers and serde for three requests
  and one reply: C6 +14,000 B on CI's builds (2,976,560 → 2,990,560 at the
  merge base vs the branch), under the plan's 16 KB stop and above its
  8 KB expectation; `nm` shows new code, no duplicated monomorphization.
  S3 +13,456 B, classic +13,872 B. Headroom on the C6 stays ~417 KB.
- The emulator walk proves the transport, UI and store over the USB shim;
  `?ble=emu` is blocked by an open defect
  (`2026-10-02-the-ble-emu-polyfill-relays-lp-link-bytes-as-m-lines.md`), so
  the Bluetooth half is proven by host tests and core tests only.

## Alternatives Considered

- **Fold into `access.json` as v4** — one write-only device file, but it
  couples two unrelated formats' version histories.
- **USB-only for a password-carrying set** (WQ3a) — safer, but G3 expects
  Bluetooth provisioning, and the fix belongs in the link (sealing), not in
  a per-request transport check.
- **Seal the password field under the login key** (WQ3b) — fails on a board
  open at edit (no key), and needs ChaCha in every image before M6.
- **Status at play** (WQ4b) — M8 may want a LAN address for play-tier relay
  clients; it decides then, as its own field.
- **Let the agent pass a password the user typed in chat** (WQ8a) — the
  password would reach the model provider.

## Follow-ups

- Seal Bluetooth links (`ble().secured()`), closing WQ3.
- M6: the station, reading the file at boot, the probe's real states.
- Fix `?ble=emu` and run `just walk-wifi-emu ble`.
- "Use the same Wi‑Fi as my other board" (a client-side store decision);
  more than one saved network; DPP from phones.
- Revisit storing the PMK (WQ2b) once M6 knows what esp-radio accepts.
