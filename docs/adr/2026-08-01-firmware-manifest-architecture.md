# Firmware manifest architecture: feature registry, self-describing builds, three-store join

- Status: accepted
- Date: 2026-08-01
- Plan: `2026-08-01-1200-firmware-manifest` (M1 + M2)

## Context

"What is this firmware" was answered four times, by hand, in four places that
drifted freely: cargo feature resolution (the truth, invisible outside the
build), `FwProvenance` on ServerHello (no features; two embedders never set
it), the studio firmware package's `manifest.json` (a **hard-coded**
`build.features` list and a `sed` extracting `WIRE_PROTO_VERSION`), and the
justfile's flash-size strings duplicated across three files. Firmware
families are multiplying (esp32c6, esp32s3, esp32v3 in progress, a
desktop-class server planned), 8/16 MB build variants are coming, and the
boards catalog needs to say what runs where. The ad-hoc approach does not
scale past two firmwares.

## Decision

### One typed feature vocabulary (M1)

`lpc_model::LpFeature` is the canonical registry of product-level firmware
features, serialized as namespaced kebab identifiers: `node.*` (engine
node-kind runtimes), `svc.*` (embedder-wired hardware services), `gfx.*`
(graphics backend), `diag.*` (diagnostics tier), `shader.*` (shader-engine
capabilities). **Feature IDs are API**: once shipped, never renamed or
reused (the Android `<uses-feature>` lesson). Wildcard-free matches
everywhere: a new variant is a compile error at every site that must
classify it — `ALL`, `wire_name`, `for_node_kind`, the engine origin
classification, the blob fragment table.

Facts come in three kinds and are never merged into one list:

- **features** — booleans, from `cfg!` (compiled-in abilities);
- **hard limits** — numbers true by construction (`ManifestLimits`:
  partition/flash/RAM facts), from build inputs like `partitions.csv`;
- **soft limits** — *measured* envelopes, property of a (build × board)
  pair, stored in a future measurement store with provenance and dates
  (roadmap M6). They never ride the firmware manifest, which is
  board-independent.

### The self-description invariant (M2)

The manifest core — package, target, features, limits, wire proto,
provenance — is derived by Rust code next to the gates and **compiled into
every firmware binary** as a magic-delimited JSON blob in a `#[used]`
static. Tooling **extracts** it (`lp-cli firmware show`,
`scripts/extract-fw-manifest.mjs`); nothing downstream re-states what a
build enabled. Go's buildinfo is the prior art: the artifact is the source
of truth about itself.

Mechanism choices, and why:

- **Const assembly, no serde in firmware.** The blob is concatenated at
  compile time by `const fn` helpers (`lp_const_concat!`,
  `lp_embed_manifest_core!` in `lpc_model::manifest`). Numbers are emitted
  fixed-width with trailing JSON whitespace; the feature array's trailing
  comma is blanked to a space. Cost is the blob's own bytes (~400 B) — no
  formatting or serialization code is linked.
- **Plain `.rodata` + byte-scan, not a custom link section.** A dedicated
  section would need per-target linker-script guarantees on four targets;
  a delimiter scan works identically on an ELF, an espflash merged image,
  and a wasm module. Delimiters embed control bytes (`\x01`…`\x04`) and are
  `concat!`-split at every source-level use so the only contiguous
  occurrence in an artifact is a real blob.
- **One derivation, many projections.** `lpc_engine::features::origin()` is
  the single classification of every feature as engine-owned (`cfg!`
  truth) or embedder-owned. `supported_features()` (runtime, for M4's
  ServerHello) and `ENGINE_FEATURE_FRAGMENT` (const, for the blob) are both
  derived from it and asserted equal by test. Embedders name only their own
  facts (gfx backend, services, f32); the macro's required named fields
  make omission a compile error.
- **Build inputs are parsed, not transcribed.** `flashAppBytes` comes from
  each chip's `partitions.csv`, parsed by its build.rs into an env var —
  the same file espflash flashes with (cf.
  `docs/debt/firmware-partition-constants-transcribed.md`).
- **CI proves it.** The firmware CI jobs extract the manifest from the
  image they just built and diff it (provenance stripped) against a
  checked-in `manifest-core.expected.json` per firmware. The expected file
  is a *review surface* — a PR that changes a build's feature set changes
  the fixture visibly.

### The three-store join

Every downstream surface is a join over three stores, each with one owner:
**build manifests** (extracted, this ADR) × **board manifests**
(`lpc-hardware`, board pins/SoC) × **measurements** (future M6). Board↔build
compatibility is *computed* (chip matches ∧ flash fits — roadmap M5), never
hand-authored. There is deliberately no fourth place where firmware truth is
typed by hand.

## Consequences

- A new firmware crate gets a correct manifest by invoking one macro; a new
  feature forces explicit classification at compile time everywhere.
- `manifest.json` v2 (roadmap M3) becomes extraction + distribution facts;
  the hard-coded `.mjs` feature list and the wireProto `sed` die there.
- ServerHello (M4) reports the same core plus runtime hardware facts —
  build facts and hardware facts stay distinct fields (a desktop build's
  GPU-vs-CPU is runtime truth, not build truth).
- The blob adds ~400 B to each image (measured against the C6 budget in
  M2's PR).
- Renaming a shipped feature ID is a breaking act and must be treated like
  a wire-proto bump.

## Rejected

- **Re-deriving the manifest in tooling from build defs** — reintroduces a
  second truth; the exact drift this kills.
- **build.rs-derived feature lists** — a crate's build.rs sees only its own
  `CARGO_FEATURE_*`, not the resolved gates of `lpc-engine` deep in the
  graph.
- **Custom ELF/wasm sections as the primary carrier** — per-target
  linker-script risk for zero extraction benefit over a scan.
- **One flat capability string list** — conflates features with limits;
  Vulkan's features/limits split is the model instead.
