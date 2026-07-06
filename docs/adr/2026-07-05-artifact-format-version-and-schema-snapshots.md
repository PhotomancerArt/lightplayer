# ADR: Artifact Format Version and Generated Schema Snapshots

- **Status:** Accepted
- **Date:** 2026-07-05
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Relates to:** `2026-07-04-json-only-artifacts.md` (JSON-only, one node
  per file — the artifact form these schemas describe)

## Context

With artifacts JSON-only and one-node-per-file, the on-disk format is now a
single canonical shape — but nothing made changes to that shape *visible*. A
slot added to a `Def`, a renamed enum variant, or a changed on-disk encoding
altered what every authored `project.json` means, and the only trace was a
Rust diff a reviewer had to interpret. There was also no version contract:
a device handed an artifact from a newer host failed with a confusing deep
parse error instead of a clear "wrong format".

Real usage is still near zero, so the cheap moment to establish format
hygiene is now. Constraints:

- **No firmware impact.** Schema tooling must not add dependencies, code
  size, or complexity to the device path (AGENTS.md; 3 MB app partition).
  `schemars`/`jsonschema` must never appear in an RV32 graph.
- The slot codec (`dynamic_slot_reader`/`writer`) parses against runtime
  `SlotShape` descriptions — a machine-readable source of truth for the
  format already exists; it just was not exported or diffed.
- Wire/protocol compatibility is explicitly NOT maintained during heavy
  development; the same stance applies to automated artifact-format compat.

## Decision

1. **Generated, checked-in format descriptions; drift is a CI failure.**
   `lp-cli schema gen` (host-only; `schema-gen` features on
   `lpc-model`/`lpc-hardware`) emits `schemas/` from the populated shape
   registry: `project.schema.json`, `node.schema.json`,
   `hardware.schema.json`, and `schemas/shapes/*.json`. Output is
   byte-stable; `just schema-check` (part of `just check`) regenerates and
   fails on any difference — the story-PNG golden-file pattern applied to
   the artifact format. Any change to the format shows up in review as a
   readable `schemas/` diff.
2. **The slot-shape dump is the source of truth; JSON Schema is a lossy
   editor projection.** Both are generated from the same registry, so
   neither can drift from the parser — but only the shape dump captures the
   codec's actual contract. Measured behaviors the schema cannot express:
   record slots have *zero* required fields (missing fields take factory
   defaults); unit payloads accept arbitrary junk (the reader skips them);
   `Ratio`/`PositiveF32` bounds are editor hints the codec does not enforce;
   the `kind`/variant discriminator must be the *first* property (streaming
   codec — JSON Schema cannot state property order); `LpValue::Any` rejects
   objects/null on read even though the writer can emit them. A future
   upgrader must therefore consume shape dumps, not JSON Schemas.
3. **Corpus conformance keeps the schemas trustworthy.** Every authored
   artifact (projects/, examples/, the fw-browser smoke project, board
   manifests — 108 files) validates against the checked-in schemas in
   `lp-cli/tests/schema_conformance.rs`, which runs in normal CI. The
   schemas are load-bearing, not decorative.
4. **`"format": N` contract on the project root.** A single monotonic `u32`
   (`PROJECT_FORMAT_VERSION` in `lpc-model`, currently 1) is required on
   `project.json`; child node files are versioned transitively through
   their root. `ProjectRegistry::load_root` probes the raw root bytes
   (streaming, before any full parse) and rejects missing/mismatched
   versions with `RegistryError::FormatVersion { expected, found }` — a
   clear "regenerate or upgrade" error instead of a deep parse failure.
5. **The device is current-version-only, by design.** An upgrader on the
   ESP32 would cost flash, RAM, and complexity for a job that never needs
   to run there. The vision: Studio/desktop tooling upgrades old JSON
   offline (device → pull → upgrade → push); the device only ever performs
   an integer compare. The upgrader itself is future work — this ADR
   records the contract it will build on.
6. **Bump ritual with history snapshots, starting at the first real bump.**
   `just format-bump` snapshots the *outgoing* schemas, shape dumps, and a
   few real fixture artifacts into `schemas/history/v<N>/` before the
   constant is bumped by hand. `schemas/history/` does not exist until the
   first bump — git covers the pre-versioning era; the point of checked-in
   history is that old shapes and fixtures become *build-time inputs* for
   the future upgrader and its tests. Automated compat classification
   (additive vs breaking) is explicitly deferred while formats churn; the
   `schemas/` diff plus the author's PR note is the procedure.

Editor integration maps files to schemas via checked-in IDE config
(`.vscode/settings.json`, `.idea/jsonSchemas.xml`); no `$schema` key is
embedded in artifact files (several defs use `deny_unknown_fields`, and it
would be dead bytes on device-shipped files).

## Consequences

- Format changes are impossible to land silently: `just check` fails on
  schema drift, and the regenerated `schemas/` diff is the review artifact.
- The firmware graph is provably untouched: `scripts/check-schemars-fw.sh`
  (also in `just check`) asserts `cargo tree -i schemars` is empty for
  `fw-esp32` and `fw-emu` on the RV32 target. The only device-path change
  is the integer format gate.
- Authoring gets autocomplete/validation in VS Code and JetBrains for free
  from the checked-in mappings.
- Old-format projects are rejected, not upgraded, everywhere today —
  acceptable while the corpus is small and regenerable; the upgrade path
  is deliberately deferred to Studio/desktop tooling.
- Generator maintenance: `SlotShape::Custom` codecs carry a hand-written
  schema side table in `lpc-model::schema_gen`; a new custom codec needs a
  table entry (the conformance corpus catches omissions that misdescribe
  authored files).

## Alternatives Considered

- **schemars derives as the schema source:** already half-wired (~40
  derives), but a parallel description that can silently diverge from what
  the codec actually parses. The shape registry *is* the parser's input;
  compiling schemas from it removes the divergence class. schemars remains
  only for `hardware.json` (serde-based, not slot-based).
- **`$schema` keys embedded in artifacts:** rejected — parse errors under
  `deny_unknown_fields`, dead bytes on device, and per-file churn on every
  format bump. IDE mapping config achieves the same DX.
- **Automated compat classification / versioned migration framework now:**
  immature tooling, heavy ceremony, and contrary to the no-compat-during-
  heavy-dev stance. Revisit when devices are fielded.
- **Shipping an upgrader (or schema validation) on device:** violates the
  no-firmware-impact constraint for no runtime benefit; the device never
  needs to read old formats if host tooling upgrades before push.
- **Starting `schemas/history/` at v1 immediately:** noise — an archive of
  the current state duplicates `schemas/`; history earns its bytes when
  there are two formats.

## Follow-ups

- The offline upgrader (Studio/desktop) consuming `schemas/history/`
  shape dumps + fixtures — future work, not scheduled.
- CI check that a `PROJECT_FORMAT_VERSION` bump lands together with a
  `schemas/history/v<N-1>/` snapshot — deferred until the first bump.
- Wire-protocol (`lpc-wire`) schema generation — out of scope here.
