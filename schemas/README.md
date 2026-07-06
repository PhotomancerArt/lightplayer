# schemas/ — generated artifact format descriptions

Everything in this directory except this README is **generated** by
`just schema-gen` (`lp-cli schema gen`) from the populated slot-shape
registry. Do not edit the JSON files by hand — regenerate them. Output is
deterministic and byte-stable, so any format change shows up in review as a
readable diff here (the story-PNG golden-file pattern, applied to the
artifact format). Decision record:
`docs/adr/2026-07-05-artifact-format-version-and-schema-snapshots.md`.

## Contents

| Path | What it is |
|---|---|
| `project.schema.json` | JSON Schema (2020-12) for a `project.json` artifact root: `kind: "Project"` plus the required `"format": N` version key. |
| `node.schema.json` | JSON Schema for any node artifact file — a `oneOf` over every registered node kind, discriminated by the `kind` field. |
| `hardware.schema.json` | JSON Schema for board hardware manifests (`lp-core/lpc-hardware/boards/**/*.json`, `/hardware.json` device override). |
| `shapes/*.json` | Serialized `SlotShape` registry dumps — the exact structure the slot codec parses against, including on-disk enum encodings. One file per registered shape; `::` in shape names flattens to `.` in filenames. |
| `shapes/_index.json` | Human name → raw shape id for every dump. |

## Shape dumps vs JSON Schemas

The **shape dump is the source of truth**; the JSON Schema is a lossy,
editor-facing projection. Both are generated from the same registry, so
neither can drift from the parser — but the codec's real contract includes
behavior JSON Schema cannot express: record fields are all optional on read
(missing → factory default), unit payloads accept arbitrary junk,
`Ratio`/`PositiveF32` bounds are unenforced hints, the `kind` discriminator
must be the *first* property, and `LpValue::Any` reads narrower than it
writes. A future offline upgrader (Studio/desktop; the device never
upgrades) will consume shape dumps and fixture files, not JSON Schemas.

## Regenerating and CI

```bash
just schema-gen     # rewrite this tree (also deletes stale generated files)
just schema-check   # verify byte-for-byte, nonzero exit on drift
```

`schema-check` runs as part of `just check`, so CI fails on drift: change
the model, regenerate, and commit the schema diff together with the code.
Two more guards keep the schemas honest:

- **Conformance:** `lp-cli/tests/schema_conformance.rs` validates every
  authored artifact (`projects/`, `examples/`, the fw-browser smoke
  project, board manifests) against the checked-in schemas in normal CI.
- **Firmware isolation:** `just lint-schemars-fw` asserts `schemars` never
  appears in an RV32 firmware graph — schema generation is host-only
  tooling behind the non-default `schema-gen` features.

## Format version and the bump procedure

`project.json` carries `"format": N` (`PROJECT_FORMAT_VERSION` in
`lp-core/lpc-model/src/nodes/project/project_def.rs`); loaders reject a
missing or mismatched version before parsing. To make a breaking format
change:

1. `just format-bump` — snapshots the *outgoing* schemas, shape dumps, and
   a few real fixture artifacts into `schemas/history/v<N>/` (the future
   upgrader's build-time inputs). The recipe refuses to overwrite an
   existing snapshot and does not edit the constant.
2. Bump `PROJECT_FORMAT_VERSION` by hand and make the format change.
3. Update the authored `project.json` files (`projects/`, `examples/`,
   `lp-fw/fw-browser/www/smoke-project`).
4. `just schema-gen`, then `just check` and `cargo test -p lp-cli`.
5. Commit the snapshot together with the bump.

`schemas/history/` does not exist yet — it appears at the first real bump.

## Editor integration

Checked-in IDE config maps artifact files to these schemas for
autocomplete/validation; artifact files carry no `$schema` key (it would be
rejected by `deny_unknown_fields` defs and is dead bytes on device).

- **VS Code / Cursor:** `.vscode/settings.json` (`json.schemas`).
- **JetBrains:** `.idea/jsonSchemas.xml`. Note: JetBrains patterns cannot
  exclude, so `project.json` files match both the node and project-root
  mappings; if the IDE asks, pick "LightPlayer project root".
