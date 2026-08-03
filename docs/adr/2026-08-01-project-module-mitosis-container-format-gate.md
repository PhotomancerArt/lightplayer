# Project/module mitosis: container manifest and the relocated format gate

- Status: accepted
- Date: 2026-08-01
- Context: `docs/design/modules.md` §1/§6 (ratified 2026-07-31);
  implementation plan `planning/2026-08-01-1003-modules-impl-roadmap` P2.

## Decision

The authored project root splits into two files ("mitosis"):

- **`project.json` — the container manifest, NOT a node.** Exactly the
  workspace identity: `format`, optional `uid`, optional `name` (provenance
  joins it in a later phase). No `kind` tag, no node envelope, unknown keys
  rejected. Modeled as `lpc_model::ProjectManifest`: read by a streaming
  `JsonSyntaxSource` probe, written by a hand-rolled deterministic writer.
  It is deliberately **not** a `#[derive(Slotted)]` type — a second
  shape+codec surface would link into every firmware image for three
  fields, and serde surface is the flash lever.
- **`module.json` — the root module node** (`kind: "Module"`, `nodes`
  map). `ModuleDef` loses `format`/`uid`/`name` to the container; carrying
  them in a module artifact is now a parse error, so pre-mitosis roots
  fail loudly instead of silently dropping identity fields.

**Format gate (settled D-A).** `PROJECT_FORMAT_VERSION` bumps 2 → 3
(bump-and-refuse; v2 schemas snapshotted to `schemas/history/v2/`). The
gate moves to the container: `ProjectRegistry::load_root` reads
`/project.json` through the streaming probe before anything parses — one
code path on host, browser, and device, proven on the emulated-firmware
path. A **missing or malformed container manifest is a hard refuse**,
never a skip: the manifest carries the gate, so the old
skip-on-malformed fallthrough would let unversioned projects load
ungated. Devices receive both files; the deploy path copies the whole
folder, so no firmware-side special case exists.

**Vendored module folders carry no format** (Q10 settled): a module
folder inside a project is gated by the host project's container; the
loader never re-runs the gate for child artifacts. Standalone opening of
a bare module folder wraps it in a workbench project and assumes the
current format (alpha posture).

**Schemas.** `project.schema.json` becomes the closed container schema
(`additionalProperties: false`, `format` const-pinned); a new
`module.schema.json` is the single-variant kind-tagged envelope over
`ModuleDef`; the conformance walk routes by filename.

**Studio.** The library's `package_manifest` rewrites on
`ProjectManifest` (read→modify→write is lossless because the vocabulary
is closed). The gallery rename now also patches the manifest `name` —
post-mitosis the manifest is library-owned workspace metadata, never an
authored def slot, so rename lives where the identity lives. The project
popup's settings rows render read-only from the manifest; the root def
contributes only its `nodes` count. Blank Created packages write a
minimal `module.json` so they stay loadable — gated to blank creates so
device pulls stay byte-faithful and adoption parity hashes cannot
diverge.

## Rejected alternatives

- **Deploy-time-only gating** (validate format when Studio pushes, skip
  on device): loses the on-device refusal for projects that arrive by
  other means (copied SD contents, partial syncs), and splits the gate
  into two implementations. The streaming container probe costs the
  device one tiny file read.
- **Slotted container type**: uniform codec machinery, but drags a second
  shape and its serializer into every firmware image for three fields.
- **Keeping identity fields on the root module def**: the design's whole
  point is that workspace concerns (who/when/what version) are not part
  of the module's technical spec; it also kept the "Studio lets you
  retype your project's uid" class of bugs structurally possible.

## Consequences

- Pre-mitosis projects (format 2 and earlier) refuse with a clear
  format/manifest diagnostic; there is no migration (alpha posture).
- `read_module_format_json`/`ModuleFormatProbe` are gone; the manifest
  probe is the single format authority.
- The persisted-state and panel phases (P8+) get a stable, non-authored
  home for container-adjacent state (`.lp/`), and P3 adds provenance to
  the manifest vocabulary.
