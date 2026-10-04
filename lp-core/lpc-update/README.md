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
| `T` | the boot state can't be trusted (or the core is on trial), so no core install | — |
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
`{ kind, done, total, busy }` while a transfer is pending or running.
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

## Validation

```bash
cargo test -p lpc-update
cargo check -p lpc-update --target riscv32imac-unknown-none-elf
cargo clippy -p lpc-update --all-targets -- -D warnings
```
