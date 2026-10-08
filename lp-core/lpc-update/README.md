# lpc-update

Over-the-air update **protocol v1** for the split image: the messages of
lp-link channel 3, the board manifest, the code table, encoding 1 (the
dictionary rule), the transfer-progress record, the two hash rules — and the
board's update session, a sans-IO state machine the firmware drives.

`no_std` + `alloc`, sans-IO: no clock (time is the caller's `now_ms`), no
randomness (injected), no IO (flash is the `UpdateTarget` trait). It is a
core crate (`AGENTS.md`'s sans-IO rule) and is meant to link into the C6's
core.

**Binding:** protocol v1 binds from Part B's first release (the split
image's D11). Until then it changes here, not as versions. After it, every
format below changes **only by adding**, and a reader ignores what it does
not know. `tests/v1_golden.hex` pins v1 byte for byte: a mismatch is a
protocol break, never a golden to re-capture.

**Dependency rule:** this crate never depends on the firmware-distribution
plan's crates (`lpc-firmware-release`, `lpa-firmware-store`). The host side
is `lp-app/lpa-update`.

## Messages (`src/message.rs`)

One lp-link message per protocol message, little-endian, the first byte the
type. Kinds: `C` core, `E` engine.

| Type | Direction | Layout (after the type byte) | Meaning |
|---|---|---|---|
| `Q` | host→board | `proto:u8` | "Who are you?" Answered with `M` |
| `M` | board→host | JSON of `BoardManifest` | The board manifest. Core-only also sends it unprompted when a link comes up |
| `O` | host→board | `proto:u8 flags:u8 chip:u16 layout:u16 min_loader:u16 core_len:u32 engine_len:u32 core_sha256[32] engine_sha256[32] build_id[64]` | An offer |
| `R` | board→host | `kind:u8 off:u32 len:u32 flags:u8` | A request for one chunk. Flag bit 0: this board takes encoding 1 (`Z`) for it |
| `D` | host→board | `kind:u8 off:u32 bytes…` | A raw chunk |
| `Z` | host→board | `kind:u8 off:u32 deflate…` | One chunk of encoding 1; only for a request that set flag bit 0 |
| `G` | host→board | `kind:u8 off:u32 len:u32` | Read-back request (kind `E` only in v1) |
| `D` | board→host | `kind:u8 off:u32 bytes…` | Read-back data (the direction tells the two `D`s apart) |
| `N` | board→host | `reason:u8 detail…` | A refusal |
| `L` | both | `step:u8 …` | The core-side login: `0` begin (host); `1` challenge (board: `nonce[32] count:u8 (salt[16] iterations:u32)×count`); `2` answer (host: `count:u8 mac[32]×count`); `3` verdict (board: `tier:u8` 0 none / 1 play / 2 edit, `retry_after_ms:u32`) |

### Refusal reasons (`src/refusal.rs`) — letters are never reused

| Reason | Meaning | Detail |
|---|---|---|
| `F` | that build failed its trial here (E3) | `build_hash:u32` |
| `A` | access: log in first | — |
| `S` | doesn't fit | `need:u32 room:u32` |
| `V` | needs another chip or layout, a newer loader, or carries a must-understand flag this board does not know | `what:u8 have:u16 need:u16`; `what` 1 chip, 2 layout, 3 loader, 4 flags (`have` 0, `need` the unknown bits) |
| `B` | busy: another link holds the transfer (E6) | `done:u32 total:u32` |
| `H` | hash mismatch: the piece was dropped, or the offer's hashes contradict each other | — |
| `T` | the boot state can't be trusted, or a trial core no link has proven yet, or (for `G`) there is no valid engine to read: no writes | — |
| `U` | unknown host message | `type:u8` |

A host that does not know a reason reads it as "refused".

### The additive-only rules — forever

- **Unknown messages.** A board answers a host message type it does not know
  (and an `L` step it does not know) with `N`/`U` and the type byte, so a
  newer host learns at once that the board lacks it. A host ignores board
  message types it does not know.
- **Trailing bytes** past the fields a reader knows are ignored; a message too
  short for them is an error; writers only append.
- **Flag bits** (`src/flag_rule.rs`): in `O.flags` and `R.flags`, the low 4
  bits may be ignored; the high 4 are must-understand. A board refuses an `O`
  with an unknown must-understand bit `N`/`V` (`what` 4) before anything
  else; a host does not serve an `R` with one. v1 defines only `R` bit 0.
- **`proto`** in `Q`, `O` and `M` is information, not a negotiation: a host
  speaks the board's version and never sends a newer message to an older
  board.

## The code table (`src/code_table.rs`)

| Code | Values |
|---|---|
| `PROTO_V1` | `1` |
| `CHUNK` | `4096` (also the NOR sector) |
| chip (`u16`, in `O` only) | `1` = `"esp32c6"`. Everywhere else a chip is its word |
| layout | `1` = the split image's layout inside `factory`: loader at `+0x0000` (at most `0x5000`), progress record at `+0x5000`, boot records at `+0x6000`/`+0x7000`, core then engine from `+0x8000` to the partition's end. An install needs the offer's layout **equal** to the board's |
| loader | `0` = no version word (#903/spike loaders); `1` = the split image's loader. An install needs the board's loader **≥** the offer's `min_loader` |
| encoding | `1` = `deflate-raw` under the dictionary rule; what `R` flag bit 0 asks for, and the `id` of `ota-manifest.json`'s `encodings[]` |

`ota-manifest.json`'s `requires` uses the same integers.

## Install kind (`src/install_kind.rs`) — decided by hashes

- The offer's `core_sha256` = the running core's hash **and** its
  `engine_sha256` = the core's digest slot → an **engine install** (a heal, or
  a new core fetching its own engine).
- `core_sha256` = own but `engine_sha256` ≠ the slot → self-contradictory,
  `N`/`H`.
- Anything else → a **core install**.

The build id never decides it: it is a label, and the input of the **build
hash** (`src/build_id.rs`): CRC-32 (`lp-crc32`) of the build id's text without
its zero padding — the split image's boot-record rule. Hosts recompute it to
read `refusedBuild`.

## The board manifest (`src/board_manifest.rs`)

One serde type, JSON, camelCase. `M`'s payload on channel 3 (authoritative),
and from Part B the hello's `firmware` field (a convenience).

```json
{ "proto": 1,
  "target": "esp32c6-4mb", "chip": "esp32c6",
  "version": "2026.10.05-3", "buildId": "2026.10.05-3+abc123456789", "wireProto": 36,
  "coreSha256": "…64 hex…",   "coreLen": 1160000,
  "engineSha256": "…64 hex…", "engineLen": 1830000,
  "layout": 1, "loader": 1, "regionLen": 3375104,
  "state": "running", "refusedBuild": null, "transfer": null }
```

`state` is one of `running`, `needs-engine`, `engine-crashing`, `updating`,
`on-trial`; an unknown one reads as `Unknown`. `transfer` is
`{ kind, done, total, busy, buildHash }` while a transfer is pending or
running: the piece, bytes written and read back, the piece's length,
whether another link owns it and is live, and the build hash of the build
it installs (so a host tells "continue my update" from "another build is
pending" from the manifest alone, never from its own memory).
`engineLen` is `null` when the core cannot know it (`needs-engine` with the
header gone: the digest slot holds no length). Keys are only added, never
removed, renamed or reused; readers ignore unknown keys.

**A board running release R reports exactly the identity R's
`ota-manifest.json` publishes** (Part B tests it on the emulator, B-P08 U17):

| Board manifest | `ota-manifest.json` | Rule |
|---|---|---|
| `target`, `chip`, `version`, `wireProto` | same keys | equal |
| `buildId` | (dropped there) | equals the engine header's bytes; for a release, `version + "+" + commit[..12]` |
| `coreSha256`, `coreLen` | `core.sha256`, `core.length` | equal (the hash rules) |
| `engineSha256`, `engineLen` | `engine.sha256`, `engine.length` | equal; also the core's digest slot and Studio's cache key |
| `layout` | `requires.layout` | an install needs them **equal** |
| `loader` | `requires.loader` | an install needs the board's **≥** the manifest's |
| `regionLen` | `core.length` + `engine.length` | the board decides fit; the host only predicts it |
| `proto`, `state`, `refusedBuild`, `transfer` | — | board only |

## Encoding 1: the dictionary rule (`src/dictionary_rule.rs`)

A piece is cut into `CHUNK`-byte chunks (the last one short). Each chunk is an
**independent raw-deflate stream** compressed against a preset dictionary of
the piece's own preceding bytes, at most a 32 KiB window:

- core, chunk at `off`: `[off − 32 KiB, off)`, clamped at 0;
- engine, chunk at `off ≥ 4096`: `[max(4096, off − 32 KiB), off)`;
- engine, chunk at `off = 0` (the header sector, written last): none.

That is the whole meaning of `id: 1` in `ota-manifest.json`'s
`encodings[]`. The files — one `.z` stream per piece and its chunk-length
index, `0` meaning "no compressed form, send raw" — are described in
`ota-manifest.json` (the firmware-distribution plan, its ADR 3). There is no
container format here. The one packer is `lpa-update`'s `pack` feature.

## The transfer-progress record v1 (`src/transfer_record.rs`)

```text
 0  magic     "LPUP"
 4  version   u16 = 1
 6  kind      u8  ('C' | 'E')
 7  stage     u8  (0 = pending: the running engine accepted it; 1 = writing: core-only started it)
 8  build     u32 (the build hash)
12  dest      u32 (flash address of the piece's first byte)
16  len       u32
20  sha256    [32]
52  crc       u32 (lp-crc32 of bytes 0..52)
56  marks     ceil(len/4096) bits, LSB first; a bit programmed 0 = that chunk written and read back
```

At `factory + 0x5000` in layout 1. **Only the location is forever.** The
header is written once after an erase; marks are programmed 1 → 0 in place,
outside the CRC, each after its chunk was written and read back. Erased when
the piece commits. A bad magic or CRC is no record; a torn mark reads either
way, and both are safe.

**The foreign-record rule (part of v1):** a record is foreign when its
version is not 1, or when it is neither this core's engine transfer (kind
`E`, `build` = this core's build hash, `dest` = its engine start) nor its
pending core transfer (kind `C`, `dest` = where this core would place a core
of that `len` — never over the running core — and not the refused build). A
foreign record is ignored, then erased when the next transfer starts. That
lets an old core meet a record a newer, failed core wrote, and keeps the
shape free to change.

## The two hash rules (`src/hash_rules.rs`) — forever

- **Engine SHA-256** = SHA-256 of `engine.bin` exactly as flashed (header
  patched and committed): the core's digest slot, `ota-manifest.json`'s
  `engine.sha256`, the board's `engineSha256`, Studio's cache key, the store's
  blob key. A board accepts an engine only if its bytes hash to its digest
  slot, never to what an offer claims.
- **Core SHA-256** = SHA-256 of `core.bin` = the flash bytes
  `[core_off, core_off + core_len)`; the boot record's `core_len` is
  `core.bin`'s exact length. The core computes it at runtime (once, cached)
  and reports it as `coreSha256`; it equals `ota-manifest.json`'s
  `core.sha256`.

## The board's update session (`src/board/`)

`BoardSession` is a sans-IO state machine. The firmware (Part B) gives it
`BoardFacts` (read once at boot: chip, layout, loader, target, version,
`wireProto`, build id, the core's own SHA-256 and its digest slot, where
the core and engine are, the boot state's trust, a trial, a refused build,
the engine's status) and an `UpdateTarget` (flash erase/program/read,
fenced to the progress sector and the region, never over the running core;
placement — `core_dest`, `engine_room_for`; and the split image's format
hooks — `prepare_uncommitted_header`, `commit_engine_header`,
`write_trial_record`, `erase_engine_header` — so this crate never re-types
the split image's formats). It drives it with `link_up`, `link_down` and
`on_message(now_ms, link, bytes)` and performs what comes back: messages
per link, and `Reset`, `TrialProof` and `FlashFault` effects.

Two modes:

- **Core-only** serves every link and moves pieces. A link coming up gets
  `M` unprompted, and a trial core reports `TrialProof` (the split image's
  rule: any link coming up confirms a trial; the boot record is the
  firmware's).
- **Engine running** is the engine's channel-3 hook: `Q` → `M`, `G` →
  read-back, and an offer of another core → the progress record with stage
  *pending*, the engine header erased, `Reset` ("the flash is the
  update-pending state"). An offer of its own engine is a no-op (`M`). `D`,
  `Z` and `L` are ignored; an unknown type is `N`/`U`. The firmware passes
  the tier the engine's own login (channel 1) granted with each message;
  the session adds `OpenTo` itself. A cut between the record and the header
  erase leaves a valid engine beside a pending record: the valid engine
  boots, and its session clears the record.

**An offer is checked before anything is erased**, first failure
answering: a must-understand flag (`N`/`V` 4); the boot state (`N`/`T`);
chip, layout (equal), loader (≥) (`N`/`V`); the install kind by hashes
(`N`/`H` when contradictory); access (`N`/`A`); a trial core no link has
proven yet (`N`/`T`: a trial confirms before it fetches, because its engine
goes where the previous core still is); a core install of the refused
build (`N`/`F`); fit (`N`/`S`); another live owner (`N`/`B`).

**The core stage:** the progress record (kind `C`); the engine header
erased if it was still valid (an engine-crashing board), so a cut never
leaves a valid-looking engine over bytes the new core overwrote; each chunk
in order — erase, program, read back and compare, program its mark; then
the whole piece hashed **from flash** against the offer's `core_sha256`, the
trial record written, `Reset`.

**The engine stage** (a heal, a reinstall, or the new core fetching its
engine): the record (kind `E`); the header sector erased; sectors 1..n in
order; sector 0 held in RAM; the piece hashed — sector 0 from RAM, the rest
from flash — against the **digest slot**; sector 0 written with its commit
word cleared, read back, the commit word programmed, the record erased,
`Reset`. A hash mismatch drops the record and answers `N`/`H`.

**Resume:** at start the session reads the progress record and applies the
foreign-record rule; its own transfer resumes at the first unmarked chunk
on the next matching offer, from any link. A pending record is a transfer
at 0.

**`Z`:** a request sets flag bit 0 when the session takes encoding 1. A `Z`
for the waited chunk decodes with `lp_deflate::inflate` in a
`32 KiB + 4 KiB` window allocated at the first `Z` and re-read from flash
when out of step (after a resume, or a write that failed). A `Z` that does
not decode to exactly the chunk is asked for again raw. Inflate is never
trusted for integrity: the piece hash is. A `D`/`Z` for any chunk but the
waited one, or from a link that does not own the transfer, is ignored —
which is what lets a host send ahead.

### Access (`src/board/access_rule.rs`)

| Operation | Allowed when |
|---|---|
| `Q` / `M` | always |
| engine install (by hashes) | always (Y8): the bytes must hash to the digest slot anyway |
| core install | a trusted link (USB), or a held tier ≥ edit |
| read-back `G` | a trusted link, or a held tier ≥ play |
| resume / takeover | the rule of the transfer's kind |
| `L` (core-side login) | any link that is not keyed: a keyed link's key is its login, so `L` on one gets the verdict any login the session will not take (no tier, no wait) |

The **held tier** of an untrusted link is the highest of `OpenTo`'s tier,
the tier its core-side login granted, and a keyed link's tier.

**QY2 — still open with Yona** (may a board open to anyone nearby at Author
take a core install over radio with no password?). It is one switch,
`CORE_INSTALL_FOLLOWS_OPEN_TO`, shipping **yes** (Y2, "like WLED"): a core
install follows `OpenTo` like every other edit. **No** would make a core
install on an untrusted link ignore `OpenTo` and need a login or a key at
edit; USB and heals are unchanged either way. Both positions are tested.

**The core reads `/.lp/access.json`** (doors #14): only `secrets` and
`open`. **Changes to those fields stay additive, and no firmware migrates
the file on an unconfirmed trial boot.** A file the core cannot read
(damaged, held for a layout change, or a newer shape after a rollback)
reads as `locked()`: heals still work, and core installs over radio wait
until the board is back on a build that reads it. `AccessFacts::from_store`
is that rule.

### A secure link's key, in core-only (`src/board/core_key_lookup.rs`)

A LAN link is a secure lp-link responder whose handshake asks for a key.
With no server in core-only, the session answers it: the anonymous key
(zero salt) with the zero PSK and no grant (`open` decides); a known salt
with `lpc_access::key_candidates` over the store's secrets, best tier
first; an unknown salt refused, uncharged; a wrong guess charged to the
**login's own backoff** (one board, one backoff). The candidate that
verifies brings the link up `LinkTrust::Keyed(tier)`
(`key_authenticated`).

### A trial that hears from no host (`src/board/trial_deadline.rs`)

A trial core whose boot read a saved network, and on which no host link
comes up for three minutes, resets itself (warm): the loader fails the
trial and rolls back. The firmware drives `TrialDeadline`; the rule is
the split-image ADR's §4 amendment of 2026-10-07.

### The core-side login (`src/board/core_login.rs`)

`lpc_access::LoginState`, unchanged, over `L`: the nonce from injected
randomness (none: every login refused), the store's secrets offered by
salt and iterations, the MACs checked, the verdict with `retry_after_ms`.
One challenge per device, owned by the link that began it; a link going
down frees it. The granted tier belongs to that link for this session
only: a reset needs a new login.

### Ownership (`src/board/transfer_owner.rs`)

The link that started or resumed a transfer owns it. Another link's offer
is `N`/`B` (with `done`/`total`) while the owner is up and was heard from
within 15 s; after that, the next link to offer takes it over at the first
unwritten chunk, if it passes access. A heal while a core transfer is
pending (E2) follows the same rule and, when allowed, cancels it.

### Read-back (`src/board/read_back.rs`)

`G E off len` → one board→host `D` of at most one chunk from the engine
extent, while the engine header is valid (running, or engine-crashing so a
crashing engine can be backed up). Several `G`s may queue; each is answered
in order.

## Test support (feature `test-support`)

`src/testing/`: a NOR flash model that cuts — and tears — at any operation,
a model board (layout 1's offsets and the split image's placement rule, but
its own boot record and engine header, **documented as a model, not the
split image's formats**), and a rig that drives a `BoardSession` the way the
firmware does. This crate's tests and `lpa-update`'s host × board
simulation use it; firmware never does.

- `tests/v1_golden.rs`: the golden transcript.
- `tests/board_transfer.rs`: full updates, heals, **a power cut after every
  flash operation of a full update** (clean and torn, raw and `Z`: every cut
  converges, a bootable core exists at every frozen state, a piece cut
  mid-way asks only for the rest), every refusal before any erase, the
  install kind, foreign records, corruption, `Z` decoded against a
  dictionary read back from flash, send-ahead.
- `tests/board_access.rs`: QY2 both ways, the locked store, the login,
  ownership and takeover, the manifest's states, read-back, the running
  engine's hand-over.
- `tests/board_keyed_link.rs`: core-only's key answer (known, unknown and
  anonymous keys, the shared backoff), what a play or edit key may do, and
  `L` refused on a keyed link.

## Validation

```bash
cargo test -p lpc-update
cargo check -p lpc-update --target riscv32imac-unknown-none-elf
cargo clippy -p lpc-update --all-targets -- -D warnings
```
