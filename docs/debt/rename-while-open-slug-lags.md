---
status: carried
since: 2026-09-06
logged: 2026-09-06
area: lpa-studio-core library / project controller — renaming the open project
related: [lp-app/lpa-studio-core/src/app/studio/studio_controller.rs (`run_pending_reslug`), lp-app/lpa-studio-web/src/library_host_opfs.rs (`structural_target_uid`)]
---
# Renaming the open project moves its name now and its directory later

**Shape** — A library package has two names: the manifest `name` (what the
editor, the header and the share address show) and the dated directory
slug (`/packages/2026-09-06-1010-project`, what the gallery card titles
itself with). The gallery's rename changes both in one catalog op. That op
is REFUSED for the project open in this tab (`OpenInThisTab` — the
project-before-catalog lock rule: the open handle is chrooted to the
directory, and a live OPFS mount cannot be re-pointed at a moved directory
without storage-layer work). So a rename made while the project is open —
the project settings' name row, or the kebab on the card running in the
sim — patches the manifest through the open handle at once and queues the
directory move for the first library settle after the project closes.
Structural: the fix is a host-level "rename under an open handle" that
flushes, moves, and re-targets the mount, which the storage layer does not
offer.

**Carrying cost** — Between the rename and the close, the editor says
"Porch sign" while the gallery card still says `2026-09-06-1010-project`
(its title is the slug). With the sim keeping the project open across a
visit to the gallery, that window is the whole session. A tab closed before
the project closes keeps the manifest name and the old slug for good; the
next gallery rename fixes it.

**Workarounds** — Close the project (open another, or stop the sim) and the
slug follows on the next settle. A gallery rename of a CLOSED project still
does both halves at once.

**Incident log**
- 2026-09-06 — filed with the naming-affordances change. Design question
  parked for the visual gate: should the gallery card title the manifest
  name (falling back to the slug), which would make the lag invisible and
  retire this entry without storage work?

**Exit criteria** — Either the card titles the manifest name (the slug
becomes a directory key nobody reads), or the library host renames under an
open handle (flush → move → re-target the mount, memory host too) and
`run_pending_reslug` is deleted.
