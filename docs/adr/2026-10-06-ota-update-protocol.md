# ADR: Over-the-air updates — protocol v1 on lp-link channel 3, a core that updates itself and heals its engine

- **Status:** Accepted
- **Date:** 2026-10-06
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Plan:** `lp2025/2026-10-04-0757-ota-update-protocol` (Part A: the
  protocol and the host, PR #975/#984; Part B: the firmware, this ADR's PR)

## Context

The C6 ships as a split image (`2026-10-04-c6-split-link-firmware-loader-and-boot-records.md`,
"ADR 1"): a loader, two boot records, a **core** that brings up the
radios, the links and the filesystem, and an **engine** with the server
and the shader compiler. The split exists so that the core keeps running
while the engine — or another core — is replaced. The OTA roadmap
(`lp2025/2026-10-03-1330-ota-firmware-updates`) holds one invariant above
the rest: **a board never needs USB again** once it runs an
update-capable core. A cut at any point of an update leaves a board that
boots and can be finished or healed from wherever it is reachable.

The spike (`lp2025/2026-10-01-1854-ota-split-link-spike`, draft #903) proved
the shape on silicon over USB and BLE. This ADR records what was built from
it: the protocol, its compatibility rules, access, integrity, resume,
compression, the manifest the board reports, and the firmware's side. The
protocol's byte-level reference is `lp-core/lpc-update/README.md`; the
host's is `lp-app/lpa-update/README.md`. This ADR names the decisions and
the reasons; the READMEs and the goldens are the spec.

## Decision

### 1. Names

- A **target** is the name of a line of builds (`esp32c6-4mb`); a
  **version** is a release's name (`2026.10.05-3`); a **build id** is
  `version + "+" + commit[..12]` (JSON key `buildId`). Targets are opaque:
  no code parses a chip or a flash size out of one
  (`lp-fw/builds/README.md`, "The target").
- **The board checks facts, never names.** An install is decided by the
  chip, layout and loader codes and by hashes; the target and the build id
  are labels a host shows and compares, and the build id's CRC-32 (the
  **build hash**) names a build in the boot records and in `refusedBuild`.

### 2. Protocol v1 on lp-link channel 3

- **Channel 3** (`lp_link::CH_UPDATE`) carries it, one protocol message per
  lp-link message, the first byte the type: `Q`/`M` (who are you / the
  board manifest), `O` (an offer), `R`/`D`/`Z` (a chunk request, a raw
  chunk, a chunk of encoding 1), `G`/`D` (read-back), `N` (a refusal with
  a reason letter), `L` (the core-side login). The table is
  `lpc-update/README.md`, "Messages".
- **Channel 3 is reliable in every preset a board can carry it on**
  (`usb()`, `uart()`, `ble()`; `udp()` keeps its own mask): the host
  sends ahead and the board drops a chunk that is not its turn, which is
  only correct on a reliable, ordered channel.
- **Additive only, forever:** a board answers a host message type (or `L`
  step) it does not know with `N`/`U` and the type; readers skip trailing
  bytes they do not know and writers only append; in `O.flags` and
  `R.flags` the low 4 bits are ignorable and the high 4 must-understand
  (`N`/`V` with `what` 4); `proto` in `Q`, `O` and `M` is information, not
  a negotiation; refusal letters and message letters are never reused.
- **The install kind is decided by hashes.** An offer whose core hash is
  the running core's and whose engine hash is the core's digest slot is an
  engine install (a heal); anything else is a core install. The build id
  never decides it.
- **One code table** (chip, layout, loader, encoding) lives in
  `lpc-update`'s `code_table.rs`; `ota-manifest.json`'s `requires` uses the
  same integers. Layout 1's offsets are `lp-bootctl`'s
  (`lpc-update` names them through `lp_bootctl`, never by copying them).
- **Goldens:** `lp-core/lpc-update/tests/v1_golden.hex` pins v1's messages
  byte for byte; `lp-base/lp-link/tests/update_channel_golden.rs` pins a
  channel-3 exchange on the USB link (handshake, `Q`/`M`, a fragmented
  `D` chunk, ACKs) and that channel 3 is reliable on `usb()`, `uart()` and
  `ble()`. A mismatch in either is a protocol break, never a golden to
  re-capture.
- **Binding from this PR's first release.** Until a release ships a core
  that speaks it, v1 changes in place; after, only by adding.

### 3. The link layer is a compatibility surface (QY1, answered yes, N5)

Once a fielded core can only be reached over lp-link, "no wire
compatibility" (AGENTS.md) stops applying to the link and to channel 3:

- **Frozen:** the 4-byte header, frame kinds 0–3, channels 0–3, the
  presets' reliable masks with channel 3 reliable, the 12-byte SYN, the
  SACK bitmap, the keyed CRC-32C, COBS-FF framing, and (from Part C) BLE's
  one frame per notification. `lp-base/lp-link/tests/plain_bytes_golden.rs`
  and the channel-3 golden are the "never break" pins.
- **Growth:** new link features arrive as a SYN flag plus an extension.
  A plain receiver reads the SYN's 12-byte prefix and ignores the rest and
  any unknown flag bit (`lp-link/README.md`, rule 8;
  `tests/plain_syn_tolerance.rs`, including a 4-byte extension on a BLE SYN
  carrying channel 3).
- **BLE** carries lp-link since wire proto 37 but not channel 3 yet; it
  joins the frozen set for updates when the first update-capable core
  speaks channel 3 over BLE (Part C).
- **`secure`:** turning it on for an existing transport means hosts keep
  speaking plain to boards whose SYN says plain.
- The JSON wire on channel 1 keeps its "no compatibility" freedom: a host
  that cannot read a board's hello still sends `Q` on channel 3 (DM9).

### 4. Access

| Operation | Allowed when |
|---|---|
| `Q`/`M` | always |
| an engine install (a heal) | always, from anyone (Y8): the bytes must hash to the core's own digest slot |
| a core install | a trusted link (USB), or a held tier ≥ edit |
| read-back `G` | a trusted link, or a held tier ≥ play |

- **USB is trusted**; an untrusted link's held tier is the highest of the
  device store's `open` tier, its core-side login and (M8) a keyed link's
  tier.
- **The core-side login** is `lpc_access::LoginState` over `L`, against the
  device store's secrets. That freezes today's `lpc-access` scheme (HMAC
  over a nonce, salt and iterations per secret) on every fielded core: a
  later scheme is added beside it, never in place of it.
- **The core reads `/.lp/access.json`** — `secrets` and `open` only. Changes
  to those fields stay additive, and **no firmware migrates the file on an
  unconfirmed trial boot** (`lpa_server::access_store::may_migrate_device_store`,
  `BootStanding`): a rollback must find a file its core can read. A file the
  core cannot read is `locked()`: heals still work.
- **QY2 is still open** (may a board open to anyone nearby at Author take a
  core install over radio with no password?). It is one switch,
  `lpc_update::board::CORE_INSTALL_FOLLOWS_OPEN_TO`, shipping **yes**;
  both positions are tested. Over USB, which is all Part B ships, it makes
  no difference.

### 5. Integrity

- **SHA-256 per piece**, hashed **from flash** after the piece is written:
  a core against the offer's hash, an engine against the core's digest slot
  (never against what an offer claims). Inflate is never trusted for
  integrity.
- **The engine header goes last**, written with its commit word cleared,
  read back, then committed — so a cut never leaves a valid-looking engine
  over partial bytes. A core install erases a still-valid engine header
  first for the same reason.
- **The two hash rules, forever:** core = SHA-256 of `core.bin` = the flash
  bytes `[core_off, core_off + core_len)`; engine = SHA-256 of `engine.bin`
  exactly as flashed. The core computes its own hash at runtime and caches
  it (no slot can hold the hash of the image it sits in): **1,241 ms on
  silicon** (XIAO C6 bench board, 1,214,800 B, `sha2` at the release
  profile's `opt-level = "z"`; 814 ms on `lp-emu:esp32c6:t1`). It is taken
  once per boot, before the engine starts (the hello carries it). `sha2`
  at `opt-level = 3` measured 571 ms emulated for +12,208 B of core and was
  not taken.
- **The engine guard (DD34):** the first boot after a USB flash (no
  `confirmed` mark on its boot record) hashes the mapped engine once
  against the digest slot before entering it, and stays core-only on a
  mismatch. **1,380 ms on silicon**, 1,110 ms emulated, once per flash.

### 6. Resume

- **The progress record** (`LPUP` v1, at `0x15000`, layout 1's
  `factory + 0x5000`): kind, stage (pending / writing), build hash, dest,
  length, SHA-256, a CRC, then one mark bit per 4 KiB chunk programmed
  1 → 0 after the chunk is written and read back. **Only its location is
  forever**; a record that is not this core's own transfer is foreign:
  ignored, then erased when the next transfer starts.
- A pending record beside a valid engine is stale (a cut between the
  running engine's record and its header erase): the engine boots and the
  core erases the record before entering it.
- **Ownership:** the link that started or resumed a transfer owns it;
  another link is `N`/`B` while the owner was heard within 15 s, then may
  take over at the first unwritten chunk.
- **E2's continue-or-heal (DM16):** a host holding only the old engine
  heals it and cancels the pending core transfer; a host holding the
  pending build continues it.

### 7. Compression

- **Encoding 1** = each 4 KiB chunk an independent raw-deflate stream
  against the piece's own preceding bytes, at most a 32 KiB window (the
  engine's header sector, written last, has none). Defined in
  `lpc-update`'s `dictionary_rule.rs`; the files (`.z` streams and their
  index in `ota-manifest.json`, `schemas/ota-manifest.schema.json`) are the
  firmware distribution's (`lp-fw/builds/README.md`, "Distribution").
- **The one packer** is `lpa-update`'s `pack` feature: every chunk it packs
  is decoded back through the board's own decoder (`lp_deflate::inflate`)
  as it is packed, and `prove_piece` repeats that from the files alone.
- A board asks for `Z` per request (`R` flag bit 0); a `Z` that does not
  decode to exactly the chunk is asked for again raw. **Inflate's stack,
  measured** on the core: `UpdateEdge::on_message`'s frame is 3,328 B
  (inflate inlined) plus 672 B (`Huff<288>::new`) and 96 B; core-only's
  main-stack high-water is 9,496 B on silicon of 54,256 B (10,632–10,776 B
  emulated). Its window (36 KiB) is heap, allocated at the first `Z`.
- On silicon a whole X → Y update moved 1,826,304 B of `Z` plus 8,192 B raw
  for 3,043,714 B of pieces (0.60).

### 8. The board manifest

One serde type, `lpc_update::BoardManifest`, JSON camelCase: `M`'s
payload on channel 3 (authoritative) and, since **wire proto 38**,
`ServerHello.firmware` (a convenience; `None` on every image that is not a
split C6). Its identity fields equal what the build's `ota-manifest.json`
publishes (`lpc-update/README.md`, "The board manifest"; host side
`lpa_update::board_matches_release`); Part B's scenario U17 checks it on
the emulator and B-P11 on silicon. `state` is `running`, `needs-engine`,
`engine-crashing`, `updating` or `on-trial`. Keys are only added.

Every image's manifest core says whether it takes OTA updates: a split C6
image carries `"ota": {"layout": 1}`; a single (monolithic) image carries
no `ota` key and is updated over USB. A plain local build stays a single
image (`lp-fw/builds/README.md`).

### 9. The firmware's side (`lp-fw/fw-esp32c6/src/ota/`)

- **The running hook:** while the engine runs, its USB transport hands
  channel 3 to the core's update session (`Q`, `G`, an offer of another
  core → the pending record, the engine header erased, a reset into
  core-only).
- **Core-only:** no engine, a guard mismatch, an engine that keeps
  crashing, or a trial core → the core serves channel 3 itself (as its own
  embassy task), confirms a trial when a link comes up, takes a core or an
  engine, and heals. It speaks the login itself.
- **The update light** (the seed of the LED-indication plan): the engine
  records the first WS281x strip it opens in `/.lp/status-light.json`
  (`lpc_update::StatusLightRecord`, `format: 1`,
  `schemas/status-light.schema.json`), written only on change; core-only,
  which never parses the hardware manifest, lights up to 7 LEDs of it from
  RMT RAM: dark yellow while updating, dark red while it needs an engine,
  off otherwise.

## Consequences

**A later core may change:** anything behind an additive rule — new
message types (old boards answer `N`/`U`), new trailing fields, new
ignorable flag bits, new manifest keys, new refusal letters, new
encodings, new layouts and loaders as new code-table values, the progress
record's shape (by version: an old core treats it as foreign), new SYN
flags and extensions.

**Never:** the meaning of an existing message, letter, field, flag bit or
code; the two hash rules; the progress record's location; channel 3's
number and reliability on a preset that already carries it; the link's
frozen set (§3); the login scheme an `L` exchange runs today; the core's
reading of `secrets` and `open`.

**Costs, measured** (`just fw-esp32c6-size-check`, `esp32c6,server`;
code vs image as ADR 1 labels them): core 1,161,408 → 1,214,304 B
(+52,896 B code), engine 1,837,462 → 1,829,376 B, `app.bin` 3,051,520 →
3,108,864 B (+57,344 B image); steady headroom 357,994 → 300,544 B; update
headroom 1,015,808 → 884,736 B (main at `1f0354758` against this branch
before it merged #880; after that merge: core 1,214,800 B, engine
1,828,914 B, steady headroom 301,006 B). A core install's room is the region minus
the running core (2,161,808 B in scenario U10's `N`/`S`), so the binding
number is the steady headroom against the 64 KB floor. Boot: the
core's hash (~1.2 s on silicon) every boot before the engine starts, plus
the guard (~1.4 s) once per USB flash. Heap at the first heartbeat: +184 B
(the manifest kept for the hello).

**Silicon (B-P11, XIAO C6 bench board, 2026-10-06):** X → Y over USB with
`Z` in 67.6 s (backup 23 s, core 17 s, engine 26 s, three resets); the
read-back equal to X's `engine.bin`; `lpfs` byte-identical across the
update; 12 power cuts across both pieces, all converging to Y with every
first boot reachable and none needing USB; a heal from the host's cache in
27.1 s with no login.

## Alternatives considered

- **Precompile on the host / a dual-bank whole-image OTA:** rejected by
  ADR 1 — the flash cannot hold two images, and the compiler is the product.
- **The core's hash in the boot record:** would have changed ADR 1's
  format for a value the core can compute (DM24).
- **Negotiating `proto`:** a host speaks the board's version instead; the
  additive rules make a negotiation unnecessary and keep old cores simple.
- **The update on channel 1 (the JSON wire):** that wire changes freely
  with every build; a fielded core must be reachable by a Studio it has
  never met.
