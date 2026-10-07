# lpa-update

The host side of an over-the-air update, protocol v1 (`lp-core/lpc-update`
defines the protocol and the board's session). Studio (wasm, M7) and
`lp-cli` (Part B; the firmware distribution's packaging) share it.

**Sans-IO.** `no_std` + `alloc` in its own code (except under `pack`): no
clock (time is the caller's `now_ms`), no IO, no executor. It links
`lpa-devices` for M1's version comparison, which brings `std` in on host
and wasm; nothing here does IO. It emits decisions, stages and effects,
never UI actions or copy (DM31): whether an offer becomes a one-click or a
two-click button is `lpa-studio-core`'s (M7).

**Dependency rule:** never `lpc-firmware-release` or `lpa-firmware-store`
(the firmware distribution's crates). Engine sources are effects the edge
resolves; the edge to `lpc-firmware-release` (`HostBuild::from_ota_manifest`)
is Part B's (B-P02).

## What it holds

| Module | What |
|---|---|
| `board_view` | A board's `M` and the facts derived from it; `BoardView::absent()` for a board that said nothing (E9) |
| `host_build` | A build the host can serve: identity (target, chip word, version, build id, `wireProto`, `requires.layout`/`loader`), `core.bin`, `engine.bin`, their hashes, and each piece's encoding 1. `HostBuild::for_heal` serves a heal from a manifest and an engine |
| `host_build_facts` | `HostBuildFacts`: a build without its bytes — identity, build hash, each piece's `sha256` and `len`. All `decide()` reads. `HostBuild::facts()`, or `HostBuildFacts::from_parts` from a manifest's fields |
| `serve` | `ServeSession`: answers `R` |
| `backup` | `BackupSession`: reads the running engine back |
| `login` | `LoginClient`: answers a core's `L` challenge |
| `decide` | `decide()`, the decision; `EngineSource`, where an engine comes from |
| `drive` | `UpdateDriver`: a whole update or heal over one board's links; `UpdateIntent` and `decide_for_intent`, what the person asked for |
| `pack` (feature) | **The one packer** of encoding 1, and its prover |

## Serving (`serve/serve_session.rs`)

- `offer()` is the build's `O` (flags 0; the chip's `u16` from the code
  table). `proto` is information: every board speaks v1.
- On `R kind off len flags`:
  - an unknown must-understand bit (high 4) → not served, reported;
  - the requested chunk is answered, then up to `ahead − 1` more kept in
    flight past it: `ServeConfig::USB` is 1, `ServeConfig::BLE` is 4 (the
    spike's S5c). A request for the chunk right after the previous one,
    already sent ahead, only tops the window up; any other (the first, one
    behind the stream, a repeat) restarts the stream there;
  - the engine header (kind `E`, off 0) always goes alone;
  - flag bit 0 → `Z` when the build has encoding 1 for that piece and the
    chunk's index length is non-zero, else `D`; flag clear → `D`.
- `N` → a typed `HostRefusal`: `A` `NeedsLogin`, `B` `Busy`, `U`
  `BoardLacksMessage` (the board is older than that message: never resend
  it), `F`, `S`, `V`, `H`, `T`, and `Other` for a reason this host does not
  know. Unknown board message types are ignored (hosts are the newer side).
- Counters: requests, chunks and bytes raw / encoded / duplicate.

The board drops a chunk that arrives before its turn, so the send-ahead
window assumes what channel 3 guarantees: a reliable, ordered link (DM30).
A host whose stream stalls anyway reconnects, and the board resumes.

## Backup (`backup/backup_session.rs`)

`G E off 4096`, `ahead` outstanding; `D`s in any order, duplicates
ignored; checked against the manifest's `engineSha256`; resumable after a
drop. The length is the manifest's `engineLen`: **no host ever parses an
engine header** (doors #12).

## Login (`login/login_client.rs`)

`lpc_access::derive_login_key` and `LoginMac::compute` — Studio's scheme,
not a second one. Held keys (a browser's or account's, bound to a salt)
answer their own offer; then passwords, one per challenge. Credentials are
passed in each time; the crate stores none.

## The decision (`decide/decision.rs`)

The board is the compatibility authority; the decision only predicts. It
compares facts and hashes, never target names.

**`decide()` takes facts, not bytes** (DS5): `HostFacts { build:
&HostBuildFacts, user_tier, allow_downgrade }`. A card decides a board's
standing on every view without loading ~5 MB of core and engine; it builds
the facts from its build's manifest and fetches the bytes only when an
update runs:

```rust
let facts = HostBuildFacts::from_parts(
    identity, // HostIdentity: target, chip, version, build id, wireProto, requires
    HostPieceFacts { sha256: core_sha, len: core_len },
    HostPieceFacts { sha256: engine_sha, len: engine_len },
);
let decision = decide(&board, &HostFacts { build: &facts, user_tier, allow_downgrade: false });
```

First match wins:

| Board | Decision | Row |
|---|---|---|
| no manifest, or a layout/chip this host's code table does not know | `NeedsUsb` | E9 |
| a transfer another live link holds | `Busy` | E6 |
| a core transfer to the host's build (`transfer.buildHash`) | `ContinueUpdate` | E2, E5 |
| waiting for its engine (needs-engine, on trial, an engine transfer, or a core transfer to another build) | `Heal` | E1, E2, E13 |
| its engine keeps crashing | `ReportCrashing` (never an automatic heal) | E10 |
| another target | `OtherTarget` (no offer by default) | — |
| the host build's core and engine hashes | `Nothing` | — |
| another chip or layout, a loader below `requires.loader`, pieces larger than `regionLen` | `NeedsUsb` | E8 |
| `refusedBuild` is this build's hash | `RefusedBuild` | E3 |
| a newer version (M1's `FirmwareAge`), downgrade not asked | `BoardIsNewer` | E7 |
| the user holds only play | `NoUpdateForPlayOnly` | E12 |
| otherwise | `OfferUpdate` | — |

A heal is never withheld: it needs no login (Y8). `tests/decide.rs` has one
test per row.

## Intents (`drive/update_intent.rs`)

The table above is the automatic answer. A person's deliberate choice is an
`UpdateIntent` the driver carries (`DriverConfig::intent`, default `Auto`),
applied as one step after it — `decide_for_intent(decision, board, facts,
intent)` — so the table stays what it is (DS6):

| Intent | Decision | Becomes |
|---|---|---|
| `Auto` | any | itself |
| `Install` | `Heal` of the host's own build (the board runs its core) | itself: finishing |
| `Install` | `Heal` of another build (E13: an engine the host is not restoring) | the offer rows for the host's build: a core install |
| `Install` | `ReportCrashing` | the offer rows; `Nothing` (the board holds this build) stays `ReportCrashing` — that is `Reinstall` |
| `Install { allow_downgrade }` | `BoardIsNewer`, `OfferUpdate` | the offer rows, the intent's flag deciding E7 |
| `Reinstall` | `ReportCrashing` | `Reinstall`: the board's own engine, by hashes (E10) |
| `Install`, `Reinstall` | anything else | itself |

No intent overrides `OtherTarget`, `RefusedBuild` (QY2: there is no "try
again" of a refused build — the board answers `N`/`F` forever), `NeedsUsb`,
`Busy`, `NoUpdateForPlayOnly`, `ContinueUpdate`, nor a board's refusal.

- **`Install`** is Update, "Install X" and "Other version…": the press is
  the go. A downgrade is only ever `Install { allow_downgrade: true }`
  (`DriverConfig` has no other downgrade knob).
- **`Reinstall`** writes a crashing board's own engine again: the host's own
  engine when its build's hashes are the board's, else the board's engine
  through the engine source (cache → store, never a read-back), served as a
  heal. It needs no go and no backup, and the driver does it **once**: if
  the board comes back still crashing, the crash is the build's, and the
  driver stops `ReportCrashing`. On any other decision `Reinstall` is
  `Auto` — an offered update still waits for `go`.
- The backup (D2) still runs before every core install of a board that
  holds an engine (running or crashing); an engine-only install (a heal, a
  reinstall) and a board waiting for its engine have nothing to back up.

## The engine source (`decide/engine_source.rs`)

cache → store → (a backup only) read-back, as effects: `LookUpCache`,
`FetchFromStore { target, build_id, sha }` (the edge answers with
`lpa_firmware_store::fetch_engine_from_store`), then the driver's own
read-back. Every source's bytes are verified by the engine hash rule;
`KeepInCache` follows any source but the cache. A miss everywhere is E13.

## The update driver (`drive/update_driver.rs`)

```text
link up → Q → M → decide → the intent
  ├─ Heal, Reinstall → the host's engine, or the engine source → offer →
  │     serve until the board resets
  ├─ ContinueUpdate  → offer the host's build → serve
  ├─ OfferUpdate (after go; Install is the go) → backup (if the board holds
  │     an engine) → offer → (reset) Q → M updating →
  │     login on N/A → serve the core → (reset) M on-trial → offer → serve
  │     the engine → (reset) M running
  └─ others          → report and stop
```

Every reset is a link down and a new link up, and every link starts with
`Q`: the driver resumes from the manifest, never from its own memory (DM9).
Effects: `Send`, `NeedCredentials`, the engine-source effects,
`Progress { stage, done, total }` with the stages `BackingUp`, `Updating`,
`Restoring`, `Finishing`, the `Decided` decision, and `Done`. A running
engine's `N`/`A` stops it with `NeedsEngineLogin` (that login is channel
1's) — at once on a link the board never answered, but after a drop it is a
race with the caller's login on the new link, so a driver the board already
answered keeps its backup and asks again (`Q`) every second for up to 30 s
before it stops; a core-only board's starts the core-side login.

## The host × board simulation (`tests/sim/`)

CI's oracle for the protocol (DM26): `UpdateDriver` against `lpc-update`'s
`BoardSession` on its NOR model, through a pipe that drops links,
reorders a window, duplicates and corrupts chunks, and a board whose power
is cut after any flash operation or after the host served its Nth request.
`tests/sim/intents.rs` runs the intents against the same board: a reinstall
written once over a crash that is the build's, `Install` past E13 (with a
cut after every flash operation), past a crash, down to an older build, and
stopped by a refused build with nothing sent but `Q`. Every case asserts the end build (or the expected report), a bootable core
at every cut, resume rather than restart after a mid-piece cut, and that
the backup the host kept is the old engine. `tests/v1_board_fixture.rs`
drives the host with the v1 golden's board lines: an old core must take a
new one.

## The one packer (`pack`, feature `pack`)

`pack_piece(kind, piece) -> EncodedPiece` is **the only thing that
compresses firmware**. Its output is exactly encoding 1 of
`ota-manifest.json`: the `.z` stream (chunks back to back, no header) and
its chunk-length index (`0` = no compressed form, send raw; the index sums
to the stream's length and has one entry per 4 KiB chunk). Each chunk is a
fresh raw-deflate compressor at level 9 (`flate2` on the `zlib-rs` backend:
a dependency, Zlib licence), primed with the dictionary rule's bytes, and
**decoded back through `lp_deflate::inflate`, the board's decoder** — a
mismatch panics. The same piece gives the same bytes. `prove_piece` is the
round trip from the files alone, naming the first chunk that fails (the
firmware distribution's `release-check`). The firmware distribution's
`lp-cli firmware package` calls it to write the `.z` files and fill
`encodings[]` entry `id: 1`.

## Validation

```bash
cargo test -p lpa-update
cargo test -p lpa-update --features pack
cargo check -p lpa-update --target wasm32-unknown-unknown
cargo clippy -p lpa-update --all-targets --features pack -- -D warnings
```
