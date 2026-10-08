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

## Amendment (2026-10-06): the boot hashes run on the SHA accelerator

The core's own hash (§5, every boot) and the engine guard (DD34, the first
boot after a USB flash) no longer run `sha2` in software. Both feed the C6's
SHA block (esp-hal's `Sha`; `fw-esp32c6/src/ota/hw_sha.rs`). The core reads
itself through the cache: it maps its own extent into the MMU entries just
below the engine window, checks they are unused first, and unmaps them after
(`ScratchWindow` in `ota/engine_window.rs`). The ROM's 64-byte SPI1 reads
stay as the fallback. With the accelerator alone, the reads were 392 of the
515 ms the hash still took on silicon.

| | before | after |
|---|---:|---:|
| core hash, silicon | 1,243 ms | 157 ms |
| engine guard, silicon | 1,387 ms | 235 ms |
| power-on boot → first hello, silicon (host clock) | 1,501 ms | 712 ms |
| first boot after a flash → first hello, silicon (host clock) | 2,797 ms | 564 ms |
| normal boot → first hello, `lp-emu:esp32c6:t1` / `t3` | 1,031.5 / 1,249.2 ms | 270.3 / 430.1 ms |
| first boot after a flash → first hello, `t1` / `t3` | 2,143.0 / 2,635.3 ms | 351.8 / 684.9 ms |

The silicon figures are from the XIAO C6 bench board `A0:F2:62:87:B4:8C`,
2026-10-06. The emulated figures are from lp-emu at `208ce933f`. A power-on
hello is now bounded by USB enumeration and the host opening the link, not
by the firmware. `t3`, which charges the cache's line fills, lands within 7%
of silicon on both hashes (165 and 252 ms).

The hash rules, the hello and `M`, and every on-flash format are unchanged:
`coreSha256` is still SHA-256 of `core.bin`, and the emulator's boot and OTA
gates check that. The cost is +2,112 B of core (1,214,800 → 1,216,912 B).
Steady headroom went from 301,006 to 301,240 B, because the engine shrank by
234 B. The "`sha2` at `opt-level = 3`" alternative (§5) is moot.

## Amendment (2026-10-06, Part C — channel 3 over Bluetooth)

Part C (folded into M7's PR-3) carries channel 3 over BLE. What it added,
none of it a change to the protocol or to a format:

- **BLE joins the frozen set.** A pinned channel-3 exchange over
  `LinkConfig::ble()` (Datagram framing, one frame per write or
  notification) sits beside the USB golden in
  `lp-base/lp-link/tests/update_channel_golden.rs`, under the same "never
  re-capture" rule. The line above ("not channel 3 yet") is superseded.
- **Core-only serves every radio link**, each `Untrusted`, and opens them
  with a receive window of 32 (the engine's radio links keep the preset's
  8; the host sends at most 16 in flight). The window is decided once per
  boot on the radio port before any link can open (`RadioLinkMode`), so
  no link races the decision.
- **While the engine runs**, the link mux passes a radio link's channel-3
  message to the core's session with the tier a **login or key** granted
  it on the engine's server (`LpServer::link_granted_tier`, never the
  `open` setting) and with the device's `open` setting as it stands then
  (`LpServer::device_open`): the session applies its one access rule, QY2
  included, and starts again on a lock or unlock made since boot.
- **Core-only keeps the Wi-Fi controller**: dropping it took Bluetooth off
  the air (defect
  `docs/defects/2026-10-06-core-only-drops-the-wifi-controller-and-bluetooth-goes-dark.md`).

Proven by host tests (`fw-esp32-common` radio-link, `lpa-server`'s
`access_gate.rs` under both QY2 positions) and on the fixture C6 over
Bluetooth from Mac Chrome (refusal, X→Y with `Z`, a power cut that
resumed, a heal with no login, five in a row, one with no USB host): the
record is in `docs/reports/2026-10-06-ota-iphone-walk.md`.
## Amendment (2026-10-06): a USB update in half the time

An lp-cli USB update, X → Y on the bench C6 (`A0:F2:62:87:B4:8C`), took
**67.5 s** and now takes **32.1 s** (two runs each, same desk, same host).
espflash writes the same image in about 14.5 s. The board's own profile, the
`[OTA] timing` lines (`fw-esp32c6/src/ota/update_timing.rs`), showed where
the time went. Four changes, none of them on the wire:

Stage times are the host's (from the start, or the reset that began the
stage, to the reset that ended it), runs 1 and 2:

| stage | before | after | what moved it |
|---|---:|---:|---|
| backup (read-back of X's engine) | 21.1 / 21.2 s | 10.4 / 10.3 s | the read-back streams (below), and USB pulls four `G`s ahead |
| core (1.22 MB), to its reset | 18.9 / 18.4 s | 8.7 / 8.7 s | block erase, the lookup-table inflate, the accelerator hash, four ahead |
| engine (1.83 MB), to its reset | 27.1 / 28.4 s | 12.6 / 12.7 s | the same |
| last reset → `UpToDate` | 0.5 / 0.5 s | 0.5 / 0.5 s | — |
| **total** | **67.5 / 68.5 s** | **32.1 / 32.1 s** | |

- **Whole 64 KiB blocks are erased ahead of their chunks.** A sector erase
  cost 20–22 ms of every 4 KiB chunk, a third of the update. The C6's part
  erases a 64 KiB block in ~93 ms, against ~330 ms for its sixteen sectors.
  A chunk that starts a block the piece wholly holds, none of it written,
  erases the block (`UpdateTarget::block_size`/`erase_block`), and the
  block's chunks are then programmed without an erase. What is known erased
  is RAM only (`Transfer::erased`). A cut, a resume, a fault or a read-back
  mismatch forgets it, so an unmarked chunk is still written again from an
  erase. The fence, header-last, the commit word and the order inside a
  chunk (program → read back → mark) are unchanged. The cut-after-every-
  flash-operation tests run on a model that erases blocks and on one that
  does not.
- **The piece's SHA-256 runs on the accelerator** (`UpdateTarget::
  sha256_flash`, `BootSha256` fed from the ROM's reads): ~1.3 s → 0.5 s for
  the core, ~1.9 s → 0.8 s for the engine. Same digest, still the check a
  piece commits on.
- **`lp-deflate` decodes Huffman codes through a 9-bit lookup table**, with
  the bit-at-a-time loop for longer codes, unused code space and a stream's
  last bits: ~7.8 → ~4.2 ms of inflate a chunk.
- **A streaming update channel is pumped between frames.** The running
  engine answered one read-back `G` per server-loop pass, so a backup went at
  the frame rate (~45 ms a sector). While channel-3 messages keep arriving
  (one within 15 ms), the USB transport keeps pumping the link for up to
  40 ms before the loop renders. The engine still renders between those
  bursts.
- **`ServeConfig::USB` is `ahead` 4**, as BLE's already was: the next
  chunks arrive while the board decodes and writes this one. Board-side, v1
  already allowed it (a chunk that is not its turn is ignored; `G`s may
  queue).

Protocol v1, lp-link and every on-flash format are unchanged: no flag, no
version, no new message. The core grew by 3,600 B (1,219,072 →
1,222,672 B), and the split image's steady headroom is 300,714 B.

## Amendment (2026-10-07, OTA Wi-Fi PR A — channel 3 over the LAN)

Plan `lp2025/2026-10-06-2249-ota-wifi-updates` (PR A, P1–P6) carries
channel 3 on a board's Wi-Fi link. Protocol v1, lp-link's frames and every
on-flash format are unchanged; no `WIRE_PROTO_VERSION` bump.

- **Who may flash over the LAN: the link's key.** A LAN link is a secure
  lp-link responder, `LinkTrust::Keyed(tier)` with the tier of the key its
  handshake verified, or `Untrusted` on the anonymous key (where `open`
  decides, QY2 unchanged). §4's table holds as written: an edit key installs
  a core, play queries and backs up, anyone heals the board's own engine
  (Y8).
- **Core-only answers a LAN link's key lookup itself**
  (`lpc_update::board::BoardSession::key_lookup`, driven by
  `fw-esp32-common`'s `radio_link::core_only_links`), with the engine's
  rule, from the device store's secrets it already reads: the anonymous key
  is answered with the zero PSK and grants nothing; a known salt with its
  candidates, best tier first; an unknown salt is refused uncharged; a wrong
  guess is charged to **the session's login backoff** — one board, one
  backoff, whether the guess came as a key or as `L`.
- **`L` is refused on a keyed link** with the verdict any login the session
  will not take gets (no tier, no wait): its key is its login, as the
  engine's server refuses a `LoginAnswer` there, so an HMAC answer can never
  be relayed through a session a relay could sit in the middle of. No new
  message, no new refusal.
- **While the engine runs** the mux hands a LAN link's channel 3 to the
  update hook with the tier its key granted on the server; a relayed link's
  channel 3 is not served yet (updates through the relay are their own
  change), and core-only turns a relayed link away.
- **The LAN's update-mode window.** A LAN link opened in update mode
  advertises a receive window of 8 frames (`LAN_UPDATE_RX_WINDOW`; serve
  mode keeps 2) and the endpoint's socket receive buffer grows to hold it;
  the LAN endpoint waits for the boot's mode before it opens a link, as the
  BLE task does. `ServeConfig::LAN` is `ahead` 8.
- **A host comes back on the key it came up on.** In core-only there is no
  server, so no hello and no `LoginBegin` to learn a password's key from:
  lp-cli keeps the key an engine session verified and dials the board again
  with it after each reset (`LanLink::open_for_update`), at the address it
  was given and then at the board's `lp-xxxx.local`.

Measured on silicon (2026-10-07, FC6 fixture-c6 `A0:F2:62:87:B4:8C` on the
desk's test access point, lp-cli on a Mac on the same network, image at
`2c42254ac`), host wall time:

| run | backup | core | engine | each reset, back on the LAN | total |
|---|---:|---:|---:|---:|---:|
| Wi-Fi, X → Y with a backup (2 runs) | 20.5 / 20.5 s | 14.1 / 15.5 s | 18.8 / 18.7 s | 2.4, 2.4, 0.4 s | **58.8 / 60.2 s** |
| Wi-Fi, cached engine, window 8, `ahead` 8 (2 runs) | — | 15.6 / 15.5 s | 18.9 / 18.1 s | 2.4, 2.4, 0.4 s | **42.9 / 41.4 s** |
| USB, X → Y with a backup (same desk) | 6.7 s | 11.2 s | 13.0 s | — | **31.0 s** |

The window sweep (pieces together, `ahead` 4): 38.0 s at a window of 2,
35.5 s at 8, 33.6 s at 16; at a window of 8, `ahead` 2 / 4 / 8 took 40.9 /
35.5 / 34.0 s. On this network (~15 ms round trips) the flash paces an
update and the window is room for a slower one. The backup is the Wi-Fi
run's slow stage: the running engine's LAN link keeps its serve window of
2, so the read-back moves ~90 KB/s against USB's ~270 KB/s. Two power cuts
(`board power-cycle`, one in the core piece and one in the engine piece)
resumed each piece where its record said and ended on the new build with
nobody touching the board. Emulated, every scenario of the plan's P4 passes
on `lp-emu:esp32c6:t1+net=lan` with no USB cable at all
(`lp-cli/tests/emu_ota_lan.rs`, `just test-emu-c6-ota-lan`).

## Amendment (2026-10-08, OTA Wi-Fi PR C — channel 3 through the relay)

Plan `lp2025/2026-10-06-2249-ota-wifi-updates` (PR C, P9–P10). The relay
passes sealed lp-link frames and reads none of them, so channel 3 crosses
it untouched; what changes is the board's access rule on a relayed link.
Protocol v1, lp-link's frames, the relay's frames and every on-flash format
are unchanged: no `WIRE_PROTO_VERSION` and no `RELAY_PROTO_VERSION` bump.

- **A relayed link is `LinkTrust::Relayed(tier)`** in the update session
  (`lpc_update::board`), the relay's second lock carried into §4: the
  device's `open` **never** applies through the relay, so a relayed link
  holds only its key's tier (or, while the engine runs, the server's grant
  for it). An edit key installs a core, a play key queries and backs up,
  anyone with a key heals the board's own engine (Y8); "Anyone" gives a
  relayed link nothing, whatever the board is open to.
- **Core-only serves a relayed link** as it serves a LAN link: it answers
  the handshake's key lookup from the device store's secrets (one backoff
  with `L`), and **refuses the anonymous key's lookup** through the relay
  (`NetworkPath::Relay`), uncharged, like an unknown key. `L` is refused on
  a relayed link as on a keyed one.
- **While the engine runs** the mux hands a relayed link's channel 3 to the
  update hook too, saying it is relayed (`RadioUpdate::Message { relayed }`),
  so the session holds it to its grant alone.
- **A relayed link keeps the serve window in update mode.** Its throughput
  is bounded by the board's relay leg (2 KiB of TCP receive buffer, every
  round trip across the internet), not by lp-link's window; widening the
  leg's buffer in update mode is a measured change for later, not this one.
- **The relay leg comes back after each reset from the core**, as Wi-Fi
  relay PR B built it: a reset is a fresh boot, so the board dials as soon
  as it has an address; a dropped leg (not a refusal) is redialled at once
  (emulated: the leg back within the second the walk could see).
- **Proof.** Host: `lpc-update`'s `board_keyed_link.rs` (the relay cases),
  `fw-esp32-common`'s `core_only_links` (a real secure handshake through a
  relayed slot: the key's tier, the anonymous key refused) and
  `link_mux_transport`, and `lpa-server`'s `access_gate.rs` (channel 3 on
  every link state, relayed included). Emulated: `just walk-ota-emu
  --relay` (update, the relay dropping the board mid-core, a power cut
  mid-engine). Silicon through the real relay: G2, Yona's walk.

The relay's own compatibility promise follows from this: see
`docs/adr/2026-10-06-cloud-relay.md`'s amendment of the same date.
