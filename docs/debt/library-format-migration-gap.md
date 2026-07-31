---
status: carried
since: 2026-07-08      # first breaking format change with fielded library data
logged: 2026-07-24
area: lpa-studio-core/library + lpc-model formats + share envelopes
related:
  [
    "feature/schema-shape-gen branch (format:1 gate, checked-in schemas — the planned exit)",
    "../adr/2026-07-14-wire-hello-versioning.md",
    "../adr/2026-07-28-share-envelopes.md",
  ]
---
# Durable authored data has no format migration

**Shape** — The no-compat-during-heavy-dev policy deletes old wire and
file formats outright, but that policy is written for peers deployed in
lockstep. Several surfaces carry **durable authored data** that outlives
the build that wrote it: the LIBRARY (projects created before a `feat!`
format change keep their old bytes forever — the library never migrates,
only history accumulates) and, since 2026-07-28, share envelopes pasted
into someone's notes app or chat log. When the engine's parsers tighten,
those projects fail node-by-node with parser errors that name the
grammar, not the remedy ("binding ref must start with `bus:` or
`node:`"), and nothing marks the project as old-format in the gallery.

**Carrying cost** — Every breaking format change silently invalidates
some slice of the user's library; the failure surfaces later, in the
editor, per-node, looking like an engine bug (2026-07-24: mistaken for
an M4 regression at the gate walk). Diagnosis requires format
archaeology (git -S on the parser string).

**Workarounds** —
- Diagnose: `git log -S "<parser error text>"` dates the format break;
  compare the project's created/remixed date.
- Fix a project in place: edit the offending file in the Studio asset
  editor (e.g. prepend `bus:`/`node:` to binding refs) — the overlay/
  save flow banks history; or re-remix from the current example (the
  old project stays banked).

**Surfaces needing format checking** — verified against the tree
2026-07-28. "Today" is what actually happens now, not what should:

| Surface | Version marker | Today |
|---|---|---|
| `project.json` root | `format` (`PROJECT_FORMAT_VERSION` = 1) | `ProjectRegistry::check_root_format` rejects a mismatch at load — but an **unreadable or unparseable** root passes the gate (`Ok(())` on every early return) |
| Child node defs | none of their own; versioned transitively through the project root | no independent check — a node file moved between projects carries no version at all |
| Zip import | the archive's `project.json` | **no format check before install**: `import_zip` reads `uid`/`name` and installs; a stale archive lands in the library and fails later, per-node |
| `lp.package` envelope | `format: 1` | rejects a mismatch loudly, no migration (2026-07-28) |
| `lp.node` envelope | `format: 1` | rejects a mismatch loudly, no migration (2026-07-28) — and, like child defs, carries no engine-format version of its own |
| `/.lp/meta.json` | none | lenient parse; damage reads as absent (deliberate — it is a best-effort sidecar) |
| History event log (`EventKind`) | none | additive-only by convention; an unknown variant fails the whole log parse |
| Wire protocol | `WIRE_PROTO_VERSION` | hello handshake, lockstep peers — **not** part of this burden |

The pattern across the gaps: version markers exist at the **project
root** and at the **envelope**, and nowhere in between. Anything that
moves a single node or a single asset between projects — node copy/paste,
and the shader sharing it exists for — travels with no engine-format
version whatsoever, so a node authored against an older grammar pastes
cleanly and fails at load.

**Incident log**
- 2026-07-08 — URI-style binding refs (`feat!` 7585e653e) break
  pre-existing binding data.
- 2026-07-24 — a 2026-07-10 remix (made on a pre-change branch build)
  fails every bound node at the M4 gate walk; mistaken for a runtime-
  pool regression; root-caused to the 07-08 break. First user-visible
  hit — enabled, ironically, by D29 finally showing device projects in
  an editor.

- 2026-07-28 — share envelopes (`lp.package`, `lp.node`) add two more
  unmigrated durable formats, consciously: they carry `format: 1` and
  refuse a mismatch rather than migrating it
  (`../adr/2026-07-28-share-envelopes.md`). Yona, on being asked whether
  to build migration now: *"we're still officially in alpha state, and I
  think in the future we should try to maintain a format, but we're just
  too heavy devving right now to worry about that."* The table above is
  the enumeration that decision asked for.

**Exit criteria** — The `format:1` gate work (feature/schema-shape-gen,
unmerged): projects carry a format version, Studio/desktop MIGRATE
library data forward on open (devices never upgrade — Studio re-pushes
migrated data), and pre-gate projects get a one-time adoption path. A
project too old to migrate shows an honest card/pane state naming the
remedy, not a parser error.
