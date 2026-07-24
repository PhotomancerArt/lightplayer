---
status: carried
since: 2026-07-08      # first breaking format change with fielded library data
logged: 2026-07-24
area: lpa-studio-core/library + lpc-model formats
related:
  [
    "feature/schema-shape-gen branch (format:1 gate, checked-in schemas — the planned exit)",
    "../adr/2026-07-14-wire-hello-versioning.md",
  ]
---
# Library projects have no format migration

**Shape** — The no-compat-during-heavy-dev policy deletes old wire and
file formats outright, but the LIBRARY is durable user data: projects
created before a `feat!` format change keep their old bytes forever
(the library never migrates; only history accumulates). When the
engine's parsers tighten, those projects fail node-by-node with parser
errors that name the grammar, not the remedy ("binding ref must start
with `bus:` or `node:`"), and nothing marks the project as
old-format in the gallery.

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

**Incident log**
- 2026-07-08 — URI-style binding refs (`feat!` 7585e653e) break
  pre-existing binding data.
- 2026-07-24 — a 2026-07-10 remix (made on a pre-change branch build)
  fails every bound node at the M4 gate walk; mistaken for a runtime-
  pool regression; root-caused to the 07-08 break. First user-visible
  hit — enabled, ironically, by D29 finally showing device projects in
  an editor.

**Exit criteria** — The `format:1` gate work (feature/schema-shape-gen,
unmerged): projects carry a format version, Studio/desktop MIGRATE
library data forward on open (devices never upgrade — Studio re-pushes
migrated data), and pre-gate projects get a one-time adoption path. A
project too old to migrate shows an honest card/pane state naming the
remedy, not a parser error.
