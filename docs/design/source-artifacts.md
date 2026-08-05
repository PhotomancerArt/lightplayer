# Source Artifacts

LightPlayer projects are authored as JSON node artifacts: **one node
definition per file, assets always in separate files**. There is exactly one
authoring format and one layout — no inline child definitions, no embedded
asset bodies, no TOML. (See ADR `docs/adr/2026-07-04-json-only-artifacts.md`
for the decision record.)

## Layout

```text
example/
├── project.json      (root artifact, kind = "Project")
├── clock.json        (child node artifact)
├── shader.json       (child node artifact)
├── fixture.json      (child node artifact)
├── output.json       (child node artifact)
├── shader.glsl       (asset file)
└── mapping.svg       (asset file)
```

The device loads `/project.json` as the artifact root. A node artifact is a
JSON object whose **first field is `kind`** (the codec streams, so the kind
tag must precede the variant's fields — canonical writer output always
satisfies this):

```json
{
  "kind": "Module",
  "format": 1,
  "name": "example",
  "nodes": {
    "clock": { "ref": "./clock.json" },
    "shader": { "ref": "./shader.json" }
  }
}
```

The project root also carries a top-level `format` key: the monotonic
artifact format version (`PROJECT_FORMAT_VERSION` in `lpc-model`). Loaders
reject a root whose `format` is missing or does not match, with an error
telling the user to regenerate or upgrade the project. Child node files carry
no format key — they are versioned transitively through their project root.

Child node positions hold an invocation: either `{ "ref": "./child.json" }`
or the editing placeholder `{ "unset": {} }`. Inline definitions
(`{ "def": ... }`) and the legacy `artifact = "..."` shape are rejected at
parse time.

## Asset references

Asset slots hold a bare path string, resolved relative to the containing
artifact:

```json
{
  "kind": "Shader",
  "source": "shader.glsl",
  "render_order": 0
}
```

The object form `{ "path": "shader.glsl" }` is accepted on read for
compatibility; the canonical written form is the bare string. Inline bodies
(`{ "glsl": "..." }`, `{ "bytes": [...] }`) are rejected with an explicit
error. `source` is GLSL-specific by design — future sibling source forms
such as `wgsl` get their own field rather than overloading an anonymous
inline value.

## Canonical form

`NodeDef::write_json` emits the canonical file form: pretty-printed with
2-space indentation, fields in slot-shape declaration order (`kind` first),
map keys in model order, and a trailing newline. Output is deterministic —
identical models serialize byte-identically, so files diff cleanly in git
and device pulls match host source byte-for-byte. Overlay commits on the
device write the same canonical form.

## Format Version And Generated Schemas

The artifact format itself is described by generated, checked-in files under
`schemas/`: JSON Schemas for project roots, node artifacts, and hardware
manifests (editor autocomplete/validation via the checked-in
`.vscode/settings.json` and `.idea/jsonSchemas.xml` mappings), plus
slot-shape dumps under `schemas/shapes/` — the exact structures the slot
codec parses against, and the offline upgrader's input (below). `just
schema-gen` regenerates the tree deterministically; `just schema-check`
(part of `just check`) fails CI on drift, and a conformance test validates
every authored artifact against the checked-in schemas. Any change to the
artifact format therefore lands as a reviewable `schemas/` diff.

Format evolution is a deliberate ritual: `just format-bump` snapshots the
outgoing schemas, shape dumps, and fixture project directories into
`schemas/history/v<N>/` before `PROJECT_FORMAT_VERSION` is bumped by hand,
and scaffolds the migration step that consumes them (below). Devices never
upgrade old artifacts — they only check the version; upgrading is Studio
tooling. See `schemas/README.md` for the procedure and ADR
`docs/adr/2026-07-05-artifact-format-version-and-schema-snapshots.md` for
the decision record.

## Format Migration

`lp-app/lpa-upgrade` is the studio-only (never firmware) upgrader the
previous section's snapshots feed: it classifies a project's authored
format and migrates it forward to `PROJECT_FORMAT_VERSION` through a chain
of per-version steps, each a plain function rewriting the project's own
JSON files — tested against `schemas/history/`'s frozen fixtures, which
are the corpus, not the input a real migration runs against. It is
behavior-preserving, not improving — a migrated
project does exactly what it did before — and refuses loudly, naming what
was found and the remedy, whenever a project is below the support floor or
a shape it does not recognize. Studio applies it at every seam durable
project data crosses a format boundary: opening a library project, pulling
an old-format project off a device, and importing a zip or a pasted share
envelope. The device itself never migrates (decision 5,
`docs/adr/2026-07-05-artifact-format-version-and-schema-snapshots.md`);
device data is pulled into the library, migrated there, and pushed back.

See `lp-app/lpa-upgrade/README.md` for the crate's contract and the
step-authoring ritual, and
`docs/adr/2026-08-04-project-format-migration-architecture.md` for the
architecture decision record.

## Loading And Reloading

The project loader resolves everything through one path:

- `/project.json` is the artifact root; `ref` invocations load child node
  artifacts from the filesystem, one definition per file.
- Asset paths read files relative to the owning node artifact.

The server project wrapper stores the project filesystem and service handles.
When the filesystem reports a change inside a loaded project, the wrapper
rebuilds the engine through `ProjectLoader::load_from_root`; because there is
a single artifact form, every change re-enters the canonical loader.

The next finer-grained step is a versioned source resolver owned by the
engine: shader nodes would hold a source identity, ask the context for
`resolve_shader_source(source, last_seen_version)`, and only materialize
bytes when the source version changes. That keeps file knowledge out of
nodes while avoiding whole-project reloads for small source edits.
