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
| `project.schema.json` | JSON Schema (2020-12) for the `project.json` container manifest (not a node envelope): required `"format": N`, optional `uid`/`name` plus the optional provenance fields `author`/`version`/`license`/`created`, nothing else. |
| `module.schema.json` | JSON Schema for the `module.json` root module node artifact: `kind: "Module"` plus the compiled `ModuleDef` shape. |
| `node.schema.json` | JSON Schema for any node artifact file — a `oneOf` over every registered node kind, discriminated by the `kind` field. |
| `hardware.schema.json` | JSON Schema for board hardware manifests (`lp-core/lpc-hardware/boards/**/*.json`, `/hardware.json` device override). |
| `board-display.schema.json` | JSON Schema for board display sidecars (`*.display.json`, `lpa_boards::BoardDisplayFile`). |
| `project-access.schema.json` | JSON Schema for the project access sidecar `<project>/.lp/access.json` (`lpc_access::ProjectAccessFile`). Its **own** persisted format with its own `"version": 2` (v2 added each entry's `kind` and optional `addedAt`; v1 files still read, as `kind: password`), outside `PROJECT_FORMAT_VERSION`: `/.lp/` is not part of the project format (hash-excluded, skipped by publish/fork/zip export, write-only over every link). A serde change to it bumps its `VERSION` and ships the reader for the old one with it. |
| `device-access.schema.json` | JSON Schema for the device access store, root `/.lp/access.json` on a device (`lpc_access::DeviceAccessFile`): the device's secrets (browser keys, the account key, passwords — each with its `kind`), `bleEnabled`, `open`. Own format, `"version": 2` (v1 still reads), same rule as the sidecar. A missing store means Bluetooth on, locked, no keys. |
| `device-network.schema.json` | JSON Schema for the device network file, root `/.lp/network.json` on a device (`lpc_access::NetworkFile`): up to eight saved Wi-Fi networks in the order they were added (`networks`: `ssid`, `password`, `hidden?`; no two with one name — adding a saved name again changes its password in place), the board's Wi-Fi switch (`wifi`) and `cloudRelay` (lets lightplayer.app reach the board through the cloud relay); both switches default `true`, so a missing field means on. Own format, `"version": 1`, outside `PROJECT_FORMAT_VERSION`. **Write-only over every link at every tier** (no password leaves the board; the server answers a status without them). A missing file means no network, Wi-Fi and relay on; a damaged one reads as no network and is not rewritten until the next change. A serde change to it bumps its `VERSION`, keeps a reader for every older version (pinned old bytes in a test), and regenerates this schema; `lpa-upgrade` is not involved. |
| `ota-manifest.schema.json` | JSON Schema for `ota-manifest.json`, the OTA release manifest (`lpc_firmware_release::OtaManifest`), published on every release as `<target>.ota-manifest.json` and served at `/firmware/<target>/<release>/ota-manifest.json`. Its **own** persisted format, `"format": 1`, outside `PROJECT_FORMAT_VERSION`, and an archive door: every release keeps one forever. Readers refuse another `format` and ignore unknown fields and unknown encoding ids (an encoding entry needs only `id`); an additive field or a new encoding keeps `format: 1`. Its compatibility pin is `lp-core/lpc-firmware-release/tests/fixtures/ota-manifest.v1.json`. |
| `status-light.schema.json` | JSON Schema for the update light's record, root `/.lp/status-light.json` on a device (`lpc_update::StatusLightRecord`): `format`, `driver` (`ws281x`), `pin`, `count`, `colorOrder` — the strip the over-the-air core lights while it has no engine (dark yellow while updating, dark red while it waits for its engine). Written by the engine when a project's first WS281x output opens, only when it changed. Its **own** soft format, `"format": 1` (never `version`), device-only and outside `PROJECT_FORMAT_VERSION` (`lpa-upgrade` never sees it): readers ignore unknown keys and stay dark for an unknown driver, colour order or format. |
| `shapes/*.json` | Serialized `SlotShape` registry dumps — the exact structure the slot codec parses against, including on-disk enum encodings. One file per registered shape; `::` in shape names flattens to `.` in filenames. |
| `shapes/_index.json` | Human name → raw shape id for every dump. |

## Shape dumps vs JSON Schemas

The **shape dump is the source of truth**; the JSON Schema is a lossy,
editor-facing projection. Both are generated from the same registry, so
neither can drift from the parser — but the codec's real contract includes
behavior JSON Schema cannot express: record fields are all optional on read
(missing → factory default), unit payloads accept arbitrary junk,
`Ratio`/`PositiveF32` bounds are unenforced hints, the `kind` discriminator
must be the *first* property, `LpValue::Any` reads narrower than it writes,
and `SlotRole::Debug` fields (session-only diagnostics, e.g. the clock's
`controls.*`) are omitted from the JSON Schema entirely even though the
reader still accepts (and now warns-and-ignores) an authored value there —
the dump still carries their role, since it describes the model, not what a
def file may validly author. The offline upgrader (Studio/desktop; the
device never upgrades) is `lp-app/lpa-upgrade` — it consumes the fixture
files this directory's history snapshots, not the JSON Schemas.

## Regenerating and CI

```bash
just schema-gen     # rewrite this tree (also deletes stale generated files)
just schema-check   # verify byte-for-byte, nonzero exit on drift
```

`schema-check` runs as part of `just check`, so CI fails on drift: change
the model, regenerate, and commit the schema diff together with the code.
Two more guards keep the schemas honest:

- **Conformance:** `lp-cli/tests/schema_conformance.rs` validates every
  authored artifact (`projects/`, `catalog/`, the fw-browser smoke
  project, board manifests) against the checked-in schemas in normal CI.
- **Firmware isolation:** `just lint-schemars-fw` asserts `schemars` never
  appears in an RV32 firmware graph — schema generation is host-only
  tooling behind the non-default `schema-gen` features.

## Format version and the bump procedure

`project.json` carries `"format": N` (`PROJECT_FORMAT_VERSION` in
`lp-core/lpc-model/src/project/manifest.rs`); loaders reject a missing or
mismatched version before parsing. To make a breaking format change:

1. `just format-bump` — snapshots the *outgoing* schemas, shape dumps, and
   a few real fixture project directories (verbatim — every file, not just
   `*.json`) into `schemas/history/v<N>/`, and scaffolds
   `lp-app/lpa-upgrade/src/steps/v<N>_to_v<N+1>.rs` as a stub. Refuses to
   overwrite an existing snapshot and does not edit the constant.
2. Bump `PROJECT_FORMAT_VERSION` by hand and make the format change.
3. Update the authored `project.json` files (`projects/`, `catalog/`,
   `lp-fw/fw-browser/www/smoke-project`).
4. Write the scaffolded step's `apply()` and register it in
   `lp-app/lpa-upgrade/src/steps/mod.rs::STEPS`; copy the new snapshot's
   fixtures into `lp-app/lpa-upgrade/tests/corpus/v<N>/` and bless the
   goldens (`LPA_UPGRADE_BLESS=1 cargo test -p lpa-upgrade`) — see
   `lp-app/lpa-upgrade/README.md` for the full ritual.
5. `just schema-gen`, then `just check`, `cargo test -p lp-cli`, and
   `cargo test -p lpa-upgrade`.
6. Commit the snapshot, the corpus + goldens, and the step together with
   the bump.

A bump without a step is caught, not just documented: `lpa-upgrade`'s
`the_chain_ends_at_the_current_format` test fails the moment
`PROJECT_FORMAT_VERSION` moves past the last registered step, and a
companion test fails if `schemas/history/v<N-1>/` is missing for the
current `N` — both run in `cargo test -p lpa-upgrade`, so CI is red until
the ritual above is actually followed.

`schemas/history/` holds one directory per retired format (`v1/`, `v2/`, …),
each with that format's schemas, shape dumps, and `fixtures/<project>/`
copies of real authored artifacts — copied whole, assets included, since a
migration step needs the GLSL/SVG/map2d files as much as the JSON. Snapshots
are frozen history: never rewrite them when the model changes. Grow the
fixture list whenever a bump touches a shape the existing two fixtures don't
exercise; a real user project (sanitized — e.g. a Zook dome project) makes a
better fixture than a synthetic one, because it is guaranteed to hit shapes
an author actually reached for.

## Editor integration

Checked-in IDE config maps artifact files to these schemas for
autocomplete/validation; artifact files carry no `$schema` key (it would be
rejected by `deny_unknown_fields` defs and is dead bytes on device).

- **VS Code / Cursor:** `.vscode/settings.json` (`json.schemas`).
- **JetBrains:** `.idea/jsonSchemas.xml`. Note: JetBrains patterns cannot
  exclude, so `project.json` files match both the node and project-root
  mappings; if the IDE asks, pick "LightPlayer project root".
