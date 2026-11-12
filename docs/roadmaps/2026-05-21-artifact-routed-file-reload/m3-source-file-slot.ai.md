# Milestone 3: SourceFileSlot + SourceFileRef

## Title And Goal

Add **`SourceFileSlot`** / **`SourceFileRef`** and materialize API in the
**parallel stack** — authored slot type, resolved ref, on-demand text — without
switching production `ShaderDef` / nodes until **M6**.

## Parallel Build

- **`SourceFileSlot`** lands in **`lpc-model`** (new type + custom codec) — additive;
  existing `ShaderSource` / `ShaderDef.source` **unchanged until M6**.
- **`SourceFileRef` + materialize** land in **`lpc-node-registry`** — used by
  registry parse/harness tests, not by `lpc-engine` nodes yet.
- Fixture/shader **production defs** unchanged until **M6**; M5 uses harness defs only.

## Suggested Plan Location

`docs/roadmaps/2026-05-21-artifact-routed-file-reload/m3-source-file-slot/`

## Scope

In scope:

- `SourceFileSlot` custom codec in `lpc-model` (`$path`, shorthand string,
  extension-key inline tables: `glsl`, `svg`, …).
- `SourceFileRef` enum (file artifact / inline / future URL stub) in
  `lpc-node-registry`.
- Materialize API in `lpc-node-registry`: `{ version, text, diagnostic_name }`
  on demand; register file paths in M1 `ArtifactStore` when resolving refs.
- Tests in **`lpc-node-registry`** (+ `lpc-model` codec round-trips): TOML
  encode/decode, materialize version for file bump vs inline edit, diagnostic
  names.
- Harness-only or test-only defs using `SourceFileSlot` where needed to exercise
  the parallel stack (not production `ShaderDef` yet).

Out of scope:

- Replacing `ShaderSource` on production `ShaderDef` / `ComputeShaderDef` (**M6**).
- Replacing fixture `MappingConfig` SVG fields on production defs (**M6**).
- `ShaderNode` / `FixtureNode` compile path wiring (**M6**).
- Any `lpc-engine` changes (**M6**).

## Key Decisions

- **Parallel:** new slot type exists alongside old; production unchanged until **M6**.
- Nodes never store source text long-term; refs only in resolved slot data.
- Effective version = f(slot revision, file artifact version) for file mode.
- TOML encoding hard cut for **new** `SourceFileSlot`; all example projects at **M6**.

## Deliverables

- `lpc-model/src/slots/source_file.rs` (+ codec).
- `lpc-node-registry/src/source/` — `SourceFileRef`, materialize.
- Unit + registry integration tests.
- `ShaderSource` remains until **M6**.

## Dependencies

- M1 ArtifactStore in `lpc-node-registry`.
- M2 NodeDefRegistry (optional for end-to-end materialize tests).

## Execution Strategy

Full plan. Custom codec, ref resolution, and materialize span model + registry;
clarify parallel vs **M6** migration in phase notes.

Suggested chat opener:

> This milestone needs a full plan — SourceFileSlot codec and materialize in the
> parallel lpc-node-registry stack, without touching production ShaderDef until
> M6. I'll run the plan process then implement. Agree?
