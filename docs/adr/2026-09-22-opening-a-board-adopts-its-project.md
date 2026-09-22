# ADR: Opening a board binds or adopts its project, so `/device/<uid>` always heals

- **Status:** Accepted
- **Date:** 2026-09-22
- **Deciders:** Photomancer
- **Plan:** lp2025/2026-09-22-0927-device-open-adopts-board-project
- **Amends:** `2026-09-07-always-a-device-target-real-emu-sim.md` (D1/D51's
  promise that `/device/<uid>` is a resolver — this is the piece of the
  resolution that was missing)
- **Superseded by:** None

## Context

D51 promised that `/device/<uid>` is never an emitted address, only a
resolver: open a board, and the URL heals to the project it runs,
`/p/<slug>-prj…?on=mac:…`. `lens_route` (`router.rs`) has always been
ready to emit that address — it reads `UiLensRuntime::Device.project_uid`
— but nothing on the device-open path ever set it. `attach_lens` connects
the editor to whatever the board is already running and stops there;
`active_library_uid()` (`ProjectController::library.active`) stayed
`None` for every device lens, so the healing half of D51 never fired. The
address bar was stuck on `/device/<uid>`, the header read the storage id
("studio") instead of a project name, and a save had no library copy to
pull into.

An earlier attempt at this — connect-as-pull, July 2026 — was torn down
in `0a1b51d13` before it shipped. Yona hit the gap directly on
2026-09-22, opening `/device/mac:…` on prod and wanting a link he could
hand to someone, and getting a device address instead.

## Decision

A device open now runs a second step after `connect_running_project`
succeeds, before the project pane's own read: name what the board is
running by CONTENT, so the address, the header, and the save path all
agree on which library project this is.

**D1 — bind by content, never by name.** The board reports the canonical
hash of its own project directory (`lpc_history::hash_package`, the same
function on both sides). A library package is bound to the running lens
— no push, no engine reload, nothing sent to the board — only when its
head hash equals that number. A name match with a content mismatch binds
nothing; the mismatch is `BindOutcome::Differs` and the console names
both hashes (D4). The divergence UX itself — what a user does about a
board whose content is not at the library head — is out of scope here
and belongs to F1.

**D6 — candidate order: content before memory.** Binding tries, in
order: (1) a scan of every library head for the board's hash — a match
here is the truth regardless of what the registry remembers, because
another browser could have pushed a different library project to this
board since this one last did; (2) the registry association
(`RegisteredDevice.association.project`, "what this library last gave
this board"), checked by content like any other candidate. Neither
answers → `BindOutcome::NoCandidate`, which is the seam adoption uses.

**D2/D17 — adoption preserves the uid.** When no library package answers
for the board's content, the board's project is pulled off the wire (a
paged `ChangesSince` from revision zero — the same read a save already
makes, so nothing is unloaded and the running project keeps running) and
installed under the SAME uid its own `project.json` carries. One project
keeps one uid across every library that holds it (D17,
`2026-08-04-device-identity-anchored-in-silicon.md`); adoption is not
allowed to mint a fresh one. Two shapes refuse adoption outright, each
with a console line and no library write:

- a pulled manifest with **no uid** — minting one would make the library
  copy's `project.json` differ from the board's, which is inside the
  canonical hash, so the copy would never hash-match the thing it is a
  copy of, and the first save would trip the save-as-pull tripwire;
- a uid the **library already holds at a different version** — the head
  scan already ran and found nothing, so a copy under this uid elsewhere
  in the library is a different version of the same project: the
  divergence case (D4), which nobody may resolve silently.

The installed package carries `PackageProvenance::PulledFromDevice
{ device_uid, device_name }` and a history rooted at the pulled snapshot
— an origin event the provenance sidecar describes, then one `Saved`
event at the board's own content hash (`device_bind.rs::adopt_board_package`,
built the way an ordinary package's first open constructs one,
`transient::transient_opened_project`). That gives the adopted project
the two events its story needs and a `Saved` head to record the
association against.

**D5 — the association is recorded at the adopted head.** Once installed,
`bank_adopted_association` runs the same `CatalogOp::RecordPush` write
`bank_completed_push` uses for an ordinary push, at the content hash just
pulled — because that IS what the board was last given, even though this
tab never pushed it. This is what lets the mismatch page (D50) answer
"what is on this board" truthfully the next time content and library
part ways.

**D3 — automatic, local-only, no confirm.** Both the bind and the
adoption run without asking: nothing destructive is on the table. A bind
writes nothing at all. An adoption writes only this browser's library —
nothing is sent to or unloaded from the board, and the board keeps
running the whole time. If the write fails at any step, the open is
already connected and working; the failure is a warn-level console line
and an unnamed open, exactly the same shape `bank_completed_push`'s
best-effort bookkeeping already uses elsewhere on this path.

## Alternatives considered

**A confirmation dialog before adopting.** Rejected: nothing about
adoption is destructive or ambiguous enough to need one. It writes a new
package under a uid nothing else in the library holds, keyed to content
that is verified byte-for-byte against what the board reports. A prompt
here would just be friction in front of an operation that cannot lose
anything.

**Minting a fresh uid for the adopted copy.** Rejected: it would split
one project into two identities across libraries, which is exactly what
D17 rules out. The fresh-uid move belongs to a different situation
entirely — the "make it yours" fork offered when publishing is refused
because the uid is someone else's (F2) — and conflating the two would
make an ordinary re-open of your own board mint a new project every time
a fresh browser saw it first.

## Consequences

- The `/p/` link exists the moment a board is opened, whether or not this
  library had ever seen that project before — auto-publish (when signed
  in) is what then decides whether that link answers for anyone else.
- A board's project is discoverable from ANY browser that opens it, not
  only the one that originally pushed it — the library that opens second
  gets its own copy, same uid, `PulledFromDevice` provenance, rather than
  a dead end.
- The one case D51 already carved out — a board whose content is not at
  the library head — keeps its honest non-address; this ADR does not
  touch that UX, only removes the OTHER case ("this library has never
  seen this project") that used to collapse into the same dead end.
- `router.rs`'s module doc, `lens_route`'s inline comment, and
  `web_app.rs`'s device-route dispatch comment now describe bind-or-adopt
  instead of "no library project behind the lens has no honest address"
  — that sentence was only ever true before this ADR.

## Follow-ups

- **F1** — divergence UX: what a user does about a board whose content is
  not at the library head. Untouched by this ADR; `BindOutcome::Differs`
  already carries both hashes for it to build on.
- **F2** — publish refused because the uid belongs to someone else's
  library; the "make it yours" fork this ADR deliberately does not build.
- **F3** — unload-before-pull safety on the classic board, if a walk
  shows the paged `ChangesSince` read OOMs a live classic session. No
  `UnloadProject` call was added to make room for this; it stays a
  question for a walk to answer.
