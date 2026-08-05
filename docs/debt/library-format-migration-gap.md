---
status: paying-down
since: 2026-07-08      # first breaking format change with fielded library data
logged: 2026-07-24
area: lpa-studio-core/library + lpc-model formats + share envelopes
related:
  [
    "../adr/2026-08-04-project-format-migration-architecture.md (the paydown — lp-app/lpa-upgrade)",
    "../adr/2026-07-05-artifact-format-version-and-schema-snapshots.md",
    "../adr/2026-07-28-share-envelopes.md",
  ]
---
# Durable authored data has no format migration

**Shape** — The no-compat-during-heavy-dev policy deletes old wire and
file formats outright, but that policy is written for peers deployed in
lockstep. Several surfaces carry **durable authored data** that outlives
the build that wrote it: the LIBRARY (projects created before a `feat!`
format change keep their old bytes) and share envelopes pasted into
someone's notes app or chat log. Through 2026-08-04 there was no migration
tool at all — a format bump silently invalidated whatever slice of a
user's library predated it, and the failure surfaced later, per-node,
looking like an engine bug rather than a version mismatch.

**Paydown (2026-08-04, `2026-08-04-project-format-migration-architecture.md`,
PR #344)** — `lp-app/lpa-upgrade` (studio-only, never firmware) classifies
and migrates a project forward through chained per-version steps, and
Studio now calls it at every seam durable project data crosses a format
boundary. Resolved:

- **Silent library skip.** `LibraryStore::list()` used to `log::warn!` and
  drop a package whose manifest failed the strict parse — the gallery
  simply lost the card. `list()`/`summarize()` now cannot fail: every
  package gets a `PackageSummary` carrying a `PackageHealth` (`Ready` /
  `UpgradesOnOpen { found }` / `Blocked { headline, remedy }`), and a
  package with no readable `uid` gets a slug-derived stand-in identity
  (`derived_uid`) so it still has a card, a delete affordance, and a name
  — never disk-written, never confused with a real `uid`.
- **Zip-import gate bypass.** `import_zip` used to read `uid`/`name` and
  install with no format check at all; a stale archive landed in the
  library and failed later, per node. Both `import_zip` and envelope
  paste (`import_json`) now `gate_and_migrate` — classify, migrate if
  this build can, refuse otherwise — strictly before `install_package`,
  which stays byte-faithful by design.
- **Node-format absence.** `lp.node` share envelopes now carry
  `artifact_format: Option<u32>`, stamped with `PROJECT_FORMAT_VERSION`
  on every export. A pasted node with a missing or mismatched stamp is
  refused with a classified message (not migrated — see "Remaining"
  below), rather than pasting cleanly and failing at load with no trace
  of why.
- **Device wipe-only affordance.** A board holding a readable project at
  an old format used to classify as `HoldsUnreadableData`, whose only
  affordance was Wipe. It is now `RosterCardState::HoldsOldFormatProject`,
  which names the found/expected formats and — when the format is within
  `lpa-upgrade`'s floor — carries a direct-dispatch `Upgrade project`
  affordance: pull → migrate the associated library package → push. Wipe
  remains for below-floor or corrupt boards.
- **Migration existence, full stop.** There was no migration path for
  durable project data at all before this. There is now one path,
  reused by every consumer (`library/package_upgrade.rs::migrate_handle_to_
  current`): the library-open pre-flight and the device Upgrade verb both
  call it.

**Remaining** — two round-two items, deliberately deferred by the plan
that did the paydown above (`2026-08-04-project-format-migration-
architecture.md`, decisions 4 and 8), not overlooked:

- **Bare-node migration.** A pasted `lp.node` with a stale or missing
  `artifact_format` stamp is *refused*, not migrated — migrating a bare
  node needs the stamp to be reliably present first, which this round
  could not guarantee (older envelopes predate the field entirely).
  Trigger: a real user is stopped by pasting a stale node.
- **v1–v3 support.** `lpa-upgrade`'s floor is v4 (pre-TimeProduct); v1–v3
  predate project/module mitosis, their types are deleted, and their only
  surviving trace is `schemas/history/`'s frozen snapshots. A project
  below the floor is refused with an honest message, never guessed at.
  Trigger: a real holder of pre-v4 project data appears (none known as of
  2026-08-04).

Both remaining items get an honest refusal today (never a silent drop or
a wrong guess) — the paydown closed the *silent* failure mode everywhere,
even where it did not close the *migration* gap.

**Carrying cost (historical, through 2026-08-04)** — Every breaking
format change silently invalidated some slice of the user's library; the
failure surfaced later, in the editor, per-node, looking like an engine
bug (2026-07-24: mistaken for an M4 regression at the gate walk).
Diagnosis required format archaeology (`git log -S` on the parser
string) — this workaround is now a fallback for the two remaining items
above, not the everyday path.

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
  too heavy devving right now to worry about that."*
- 2026-08-04 — Yona hitting real silent failures on lightplayer.app, plus
  first external users (Zook, Spencer) arriving the following week with a
  format bump likely on their own critical path, tips the alpha ruling:
  the paydown above lands as PR #344.

**Exit criteria (original, met 2026-08-04)** — projects carry a format
version (already true); Studio/desktop MIGRATE library data forward on
open (device never upgrades — Studio re-pushes migrated data); a project
too old to migrate shows an honest card/pane state naming the remedy, not
a parser error. All three are now true; the entry stays open (`paying-
down`, not `retired`) only for the two remaining items above, each with
its own trigger.
