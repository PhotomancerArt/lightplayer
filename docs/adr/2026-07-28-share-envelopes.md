# ADR: Share envelopes for projects and nodes

- **Status:** Accepted (annotated 2026-08-04 — see the migration note below)
- **Date:** 2026-07-28
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

> **Cloud annotation (2026-08-06,
> `2026-08-06-cloud-service-architecture.md`).** This ADR's premise
> "there is no cloud provider and no account system" is no longer true —
> the lp-cloud service and Google accounts exist. The decision itself
> stands unchanged: envelopes remain the **anonymous, copy-semantics**
> sharing path (paste mints a fresh uid), while cloud sync is the
> **identity-preserving** path (push/pull keeps the project uid). The two
> deliberately coexist.

> **Migration annotation (2026-08-04,
> `2026-08-04-project-format-migration-architecture.md`).** "Deliberately
> not migrated" below is about the **envelope's own** `format` field (the
> shape of the JSON wrapper, still `1`, still refuse-on-mismatch, unchanged)
> — not about the **project artifact content** an `lp.package` envelope
> carries. That content now gates and migrates on import exactly like a
> zip: `import_json` classifies the embedded project's `PROJECT_FORMAT_
> VERSION` and runs it through `lpa-upgrade` before install, surfacing
> "Imported X — upgraded from format 4 to 5" rather than installing a
> stale artifact that fails later per-node. `lp.node` envelopes gained an
> `artifact_format: Option<u32>` stamp (new field, additive) naming the
> project format the node's def/GLSL were authored against; a paste with a
> missing or mismatched stamp is refused with a classified message, not
> migrated — bare-node migration needs the stamp to be universal first,
> which this round cannot guarantee (see the new ADR's decision 8 and
> follow-ups). The debt entry this ADR pointed at
> (`../debt/library-format-migration-gap.md`) is rewritten to match.

## Context

There is no cloud provider and no account system. The only ways a project
leaves one browser and reaches another are the filesystem and the
clipboard.

Zip already covers the project case: `export_package` / `import_zip`
(`lpa-studio-core/src/app/library/package_zip.rs`), an "Export zip" button
on gallery cards, and drag-anywhere import. That is the right channel for
a real handoff.

Two gaps remained after a demo walk:

1. **Small projects want a paste channel.** Attaching a zip to a chat
   message to share a forty-line project is friction out of proportion to
   the content, and the recipient cannot see what they are about to
   install.
2. **A single node has no channel at all.** Shaders are the thing people
   most want to hand each other, and a shader is a node def plus a `.glsl`
   asset — two files with a reference between them. Nothing could move
   that pair.

The node case turned out to be already anticipated:
`WireCreateNodeRequest` (`lpc-wire/src/project_command/create_node.rs`)
carries `{ file, body: Vec<u8>, assets: Vec<(LpPathBuf, Vec<u8>)>, attach }`
and its module doc states the request carries bytes "so future sources
(copy, import, examples) reuse it unchanged."

## Decision

Two JSON envelope formats, both leading with `{ "kind", "format" }`.

**`lp.package`** — a whole project:

```jsonc
{
  "kind": "lp.package",
  "format": 1,
  "name": "fyeah-sign",
  "files": {
    "project.json": { "text": "…" },
    "logo.png":     { "base64": "…" }
  }
}
```

**`lp.node`** — one node and its assets: `WireCreateNodeRequest` minus
`attach`, which the paste target supplies.

### Files are text when they can be

`ShareFile` is `Text { text }` or `Base64 { base64 }`, chosen by whether
the bytes are valid UTF-8. This is the point of the JSON channel: a shared
project is mostly `.json` and `.glsl`, and keeping those readable means a
pasted envelope can be skimmed, diffed, and hand-edited in a chat window.
Only genuinely binary files pay the base64 tax. Decoding accepts either
arm for any path.

Files live in a `BTreeMap`, so encoding is deterministic and two exports of
the same project are byte-identical.

### Versioned, and deliberately not migrated

`format: 1` is present and a mismatch is rejected outright
(`ShareError::UnsupportedFormat`, naming both versions and telling the user
to re-export). There is no migration, no tolerant decode, and no
dual-format path.

This is a conscious alpha-stage trade. Yona, 2026-07-28:

> we're still officially in alpha state, and I think in the future we
> should try to maintain a format, but we're just too heavy devving right
> now to worry about that.

Note this is a *different* posture from `AGENTS.md`'s wire policy ("delete
the old form outright"), and the difference is not an inconsistency. Wire
peers are deployed in lockstep, so an old form has no readers. A pasted
envelope is durable user data sitting in someone's notes app, and it will
outlive the build that wrote it. We still decline to migrate it during
alpha — but we refuse it **loudly** rather than silently misreading a
neighbouring version's bytes.

The surfaces that will eventually need real format checking are enumerated
in `docs/debt/library-format-migration-gap.md` rather than built now.

### The header is validated before the body

Both decoders check `kind` and `format` before deserializing the body.
Without this, pasting a node envelope into the gallery reports "missing
field `name`" instead of "that is an lp.node" — the structural error fires
first and buries the real cause. `peek_header` exposes the same
classification for callers that want to route a blob before decoding it.

### Import mints a fresh uid

Exactly as zip import does. Envelopes get shared; two libraries holding
the same `prj_…` uid would break the identity that history and device
associations key off. The source uid rides the installed package's
provenance as `PackageProvenance::ImportedJson { original_uid }`.

The history origin event gains a matching `EventKind::ImportedJson`.
Reusing `ImportedZip` was tempting and rejected: the origin event is a
permanent record, and labelling a paste as a zip import is a lie the user
can never correct.

## Consequences

- A project or node can be shared by pasting text, with no server, no
  account, and no file attachment.
- Shared text files stay human-readable, so a recipient can read a shader
  before installing it — a meaningful safety property when the channel is
  "someone sent me this".
- A `format` bump orphans every envelope already pasted into a chat log or
  a notes file. Accepted for alpha; the failure is loud and names the
  remedy.
- `PackageProvenance` and `EventKind` each gained a variant. Both are
  additive; the compiler found the two exhaustive matches
  (`home_view_builder::provenance_line`, `library_store::origin_event_for`).
- `lpa-studio-core` gains an `app/share/` module of pure byte functions —
  no IO, no clock, no randomness, so the sans-IO core rule holds. The
  clipboard lives in the web edge (`lpa-studio-web/src/clipboard.rs`).
- Clipboard reads are permission-gated and can be denied outright, so every
  paste affordance needs a manual fallback; the seam reports failure rather
  than pretending an empty clipboard.

## Alternatives Considered

- **Zip only, base64'd into the clipboard.** One codec instead of two, but
  the pasted text is opaque — the recipient cannot see what they are
  installing, which is the property that makes a paste channel worth
  having.
- **Migrating envelopes across `format` versions.** Correct eventually,
  wrong now: the authored formats are still moving weekly and every
  migration would be written against a shape that changes again next week.
- **A tolerant decoder that ignores unknown fields and guesses.** Silent
  misreads are worse than refusals, especially for content that arrived
  from someone else.
- **Reusing `EventKind::ImportedZip` for pastes.** Avoids a variant, but
  writes a falsehood into a permanent log.
- **Putting the node envelope in `lpc-wire` beside `WireCreateNodeRequest`.**
  Rejected: the envelope is a Studio-level sharing concern, and `lpc-wire`
  is `no_std` firmware-facing surface that should not grow clipboard
  formats.

## Follow-ups

- Wire-read (`FsRequest`) export for device-hosted projects that are not in
  the local library. Editor-popup export is library-backed only today.
- A size guard on node envelopes: a large binary asset base64s into
  something no clipboard should carry.
- ~~Revisit migration when the authored formats settle~~ — **partly closed
  2026-08-04**: the project *content* an `lp.package`/pasted-`lp.node`
  envelope carries now gates and migrates via `lpa-upgrade` (see the
  migration annotation above). Still open: the envelope's own `format`
  field (still refuse-only) and bare-node migration (needs the
  `artifact_format` stamp to be universal first) — see the debt entry.
