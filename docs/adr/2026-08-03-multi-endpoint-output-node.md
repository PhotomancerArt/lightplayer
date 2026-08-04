# ADR: One output node drives many wires, and an endpoint names a wire

- **Status:** Accepted
- **Date:** 2026-08-03
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Relates:** `2026-07-31-output-sink-retry-policy.md` (parking, preserved here
  per wire), `2026-07-05-artifact-format-version-and-schema-snapshots.md`
  (format 3 → 4; main's project/module mitosis took format 3 mid-flight, so
  this plan's channels-map change landed as the bump after it, not the one
  originally planned), `2026-07-27-map2d-document-architecture.md`
  (paths are what the spans are honest about)

## Context

An output node used to author one `endpoint` and drive one strip. A product
with five strands therefore needed five output nodes, five fixtures, and five
bus channels, and the fixture that already knew it had five paths had to be cut
into five fixtures to say so. `projects/test/quad-strips` is that shape: four
of everything, wired by hand, for one shader.

That is backwards for the hardware. A board hands out RMT channels from one
peripheral; a single render pass produces one flat buffer of lamps; and which
lamps go down which wire is a property of *the output*, not of the geometry.
Fixture mappings should stay portable — the same dome mapping should load on a
board with two channels or eight.

Two smaller things were tangled into the same knot:

- **`ws281x:rmt:D10` named a driver mechanism.** `rmt` is an ESP32 peripheral,
  chosen by firmware from the board manifest; a project has no business
  asserting it, and a future I2S or remote target would make the string a lie.
- **The fixture published one covering span** over its whole buffer, so nothing
  downstream could see where one strand ended and the next began without
  re-deriving the mapping.

## Decision

**The output node owns the channel→wire split, an endpoint names a target and a
wire, and the fixture tells the truth about its paths.**

### 1. `channels: MapSlot<u32, OutputChannelDef>`

`OutputDef.endpoint` is replaced by a map of channels, keyed by channel index:

```json
{ "kind": "Output", "channels": {
    "0": { "endpoint": "ws281x:local:IO18", "count": 4 },
    "1": { "endpoint": "ws281x:local:IO13" } } }
```

A map, not a list: the wire sync layer has no array vocabulary, and map keys
become path segments for free (`PlaylistDef.entries` is the precedent). The key
is the channel index and nothing else — it carries no offset.

**Count semantics.** `count` is in lamps; slices are derived cumulatively in
key order, so channel *k* starts where channel *k−1* ended. Only the
highest-keyed channel may omit its count, meaning "the remainder of the node's
control product". A single-entry map with no count therefore degenerates
exactly to the pre-channels behavior: one wire, whole buffer.

Two channels cannot both be the remainder, and a count-less channel in the
middle leaves everything after it without a start. That is refused: the engine
logs the offending channel key and the output drives **nothing** until it is
re-authored. A wrong strip lit is worse than a dark one, and this is authored
data, so it is a log, never a panic.

Counts are authored; the buffer is whatever the graph produced. When the counts
overrun the buffer the overflowing wires are clamped to what exists — loudly,
rate-limited on the extent that caused it — so the wires that do have pixels
still light. Authoring-time validation of counts, and the shared per-channel
LED cap, are a separate concern.

**`options` and `test_pattern` stay node-level**, shared by every channel: the
display pipeline is a property of the render, and the test pattern answers "is
this node wired to those strips". Per-wire diagnostics belong to the pin
discovery flow, not to the data model.

### 2. Flat slices and honest spans, not rows

The node's buffer stays flat — `ControlSampleSpan.row` is always 0. What
changes is that the fixture publishes **one span per authored path** instead of
one span covering the lot, so the strand boundaries are in the layout that
already ships to previews, probes, and the face. Slices live on the engine's
per-wire sink records, computed from `channels`; they are never in the layout
and never in an endpoint string.

The alternative — the fixture publishing real rows, one per path, with channels
referencing rows 1:1 — is deferred to a target that actually has rows
(sACN/Art-Net universes). Adopting it now would make every consumer of a
control product row-aware to describe the same flat dome.

### 3. `cap:target:config` — the middle segment is the target device

`ws281x:rmt:D10` becomes `ws281x:local:D10`, and the same flip applies to
button (`button:local:GPIO4`) and radio specs: one vocabulary, one migration.
The middle segment names **which device** the endpoint is on — `local` today,
a remote LightPlayer or a `pixlite-<id>` later. How the local device produces
the signal (RMT, I2S, whatever a board grows) is firmware's business, decided
from the board manifest.

An endpoint spec is an opaque, whole-string identity for **one wire**. Two
wires are two specs. Nothing about slicing, ordering, or lamp counts belongs in
it — that is exactly why the slice lives on the engine's sink record.

### 4. Format 4, version-and-refuse

`PROJECT_FORMAT_VERSION` goes 3 → 4 (P1 snapshotted format 2 as work started;
main's project/module mitosis landed format 3 mid-flight, so this change's own
bump is the one after it — `PROJECT_FORMAT_VERSION` and its full history now
live in `lp-core/lpc-model/src/project/manifest.rs`), and every in-repo
artifact is rewritten in the same change. Old projects are **refused, never
migrated** (the format-version ADR's posture). This is load-bearing rather
than hygiene: `ws281x:rmt:D10` still *parses* — endpoint validation is
structural — so without the version gate an old project would load happily
and then fail per-tick at open time on real hardware, which is the worst
possible place to learn about it.

### 5. Parking stays per wire

The retry-policy ADR is preserved by construction: each wire keeps its own
binding, its own handle, and its own `parked_at_generation`. A wire the board
cannot give parks alone; its siblings flush regardless; and re-authoring one
channel clears only that channel's parking. Flush errors and logs name the node
and the channel key, not just the endpoint — with N wires per node, "output
ws281x:local:IO14 failed" no longer identifies who asked.

## Consequences

- One fixture with five paths and one output with five channels is now the
  natural way to author a five-strand product; the four-nodes shape stays valid
  and `quad_output_channels` still pins it.
- A changed slice or endpoint closes and reopens that wire's channel. Providers
  reject a write shorter than the channel they opened, so a shrinking slice
  without a reopen would fail every frame forever — routine now that slices are
  authored, and rare before.
- The engine decodes each node's buffer **once** per frame and hands each wire a
  sub-slice; the decode used to allocate a `Vec` per sink per frame. The
  per-tick config diff walks the authored channels against the registered wires
  in key order without allocating.
- Wires are still written one at a time. Concurrent transmission — one
  `send_blocking_all` per frame instead of N blocking writes — is a separate
  change; this one keeps the slice→handle mapping in the provider-facing layer
  and gives each firmware channel persistent frame storage so that change is
  mechanical rather than a re-plumbing.
- Historical documents keep their period spec strings. ADRs and defect records
  written when the vocabulary was `ws281x:rmt` are not rewritten.
- **The shared per-channel cap moved to the engine seam, and raised.**
  `WS281X_MAX_LEDS_PER_CHANNEL` went 256 → 1024, enforced once in
  `EngineServices::wire_slice` (the seam every `OutputProvider` flushes
  through) rather than trusted to each provider, with a loud one-time
  warning on truncation and a second, config-time warning when an authored
  count exceeds the bound. This closes
  `docs/debt/output-channel-led-cap-silent-truncation.md` — see that entry's
  2026-08-03 incident for the full before/after (host and emulator providers
  never capped at all before this).
- **The output node's studio face** ships: a board diagram with per-channel
  pin assignment, and a "fit counts to strips" gesture (named `spread` during
  design; renamed at the G-A visual gate because "spread" did not explain
  itself — the gesture divides the node's lamp count evenly across the
  authored channels and previews the resulting per-channel counts before
  committing).
- **Verified on silicon.** One output node opened four RMT channels
  (IO18/IO16/IO14/IO2) from the desk DOM-Z-102 and drove all four wires
  byte-exact against the host oracle (per-wire CRCs `0x4233b049` /
  `0x7e3f40ad` / `0xf34d3f63` / `0xa6fe041e`; the four dumps concatenate to
  the full oracle frame), with the remainder-channel count semantics
  exercised on device.

## Alternatives Considered

**Keep one wire per node and add a "strip group" node.** Rejected: it moves the
same split one node further away from the hardware that has it, and leaves the
fixture still cut into one node per strand.

**Put the slice in the endpoint spec** (`ws281x:local:IO18@0+4`). Rejected: an
endpoint is a hardware identity used for claiming, equality, and status
lookups. Encoding a buffer range in it would make two spec strings for the same
pin compare unequal, and the range would need re-parsing everywhere the string
travels.

**Real rows now.** Rejected for this change, kept as a future target's shape —
see §2.

**`builtin` as the middle segment.** Rejected: it describes the peripheral's
relationship to the chip, not the device the project is talking to, and reads
wrong the moment a second device is addressable.

## Follow-ups

Everything originally listed here (the shared cap, the studio face, the
hardware walk) landed in this same plan — see the Consequences section above
for what shipped and where. What remains open:

- Concurrent per-frame transmission of a node's wires (`send_blocking_all`) —
  wires are still written one at a time (Consequences, above).
- Per-wire diagnostics / color discovery at the face (M7 of the
  hardware/board-selection roadmap) — the face is pre-discovery by design
  (settled Q6/G-A); swatch color-discovery mode already ships elsewhere and
  the face is its natural next home.
- A fifth concurrent RMT channel on the classic (M6 of the sibling
  multi-channel-output-architecture roadmap) — out of scope here; the board
  manifest stays the sole authority on channel count, and this plan's walk
  confirms four is the ceiling this hardware proved, never five.
