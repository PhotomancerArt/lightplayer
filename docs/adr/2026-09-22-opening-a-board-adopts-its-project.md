# ADR: Opening a board binds or adopts its project, so `/device/<uid>` always heals

- **Status:** Accepted
- **Date:** 2026-09-22
- **Deciders:** Photomancer
- **Plan:** lp2025/2026-09-22-0927-device-open-adopts-board-project
- **Amends:** `2026-09-07-always-a-device-target-real-emu-sim.md` (D1/D51's
  promise that `/device/<uid>` is a resolver — this is the piece of the
  resolution that was missing)
- **Superseded by:** None
- **Amended:** 2026-09-22, after shipping — identity-free boards are
  stamped, not refused (see *Amendment* below)

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
allowed to mint a fresh one. (A pulled manifest with **no uid** is not
refused: it is given one on the board first — see *Amendment* below.) Two
shapes refuse adoption outright, each with a console line and no library
write:

- **no `project.json`, or one that does not parse** — there is no
  manifest to carry an identity at all;
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

**D3 — automatic, no confirm.** Both the bind and the adoption run
without asking: nothing destructive is on the table. A bind writes
nothing at all. An adoption writes this browser's library, plus — only
when the board's project carries no identity — ONE file on the board: its
`project.json`, stamped with the uid (*Amendment* below). Nothing is
unloaded from the board, and the board keeps running the whole time. If
the write fails at any step, the open is already connected and working;
the failure is a warn-level console line and an unnamed open, exactly the
same shape `bank_completed_push`'s best-effort bookkeeping already uses
elsewhere on this path.

## Amendment (2026-09-22, after shipping)

**What prod showed.** Opening a board running
`catalog/projects/playful-choker`, pushed straight from the repo, logged
"…is running a project with no identity of its own, so it was not added
to your library" and the address stayed `/device/mac:…`. Every
`catalog/` project and every bundled example carries a uid-free
`project.json` by design — a uid is minted when a project ENTERS a
library — so the decision as shipped, which REFUSED to adopt a uid-free
manifest (minting a uid only in the library would make the copy's
`project.json` differ from the board's, inside the canonical hash), was
not refusing an edge case: it was refusing the common case, and the
healing D51 promised did not happen for most boards.

**The decision now: stamp the identity onto the board.** When the pulled
manifest parses and carries no uid, adoption gives the BOARD the
identity, so board and library copy stay byte-identical and the bind
matches by construction:

1. **Re-stamp an existing identity when the library already has this
   project.** A library package whose `project.json` is the board's plus a
   uid and nothing else, and whose head is exactly the board's files with
   that manifest substituted in, is this project: its uid is the identity,
   and its manifest bytes are what the board gets. Re-pushing from
   `catalog/` strips the uid, not the content — this is what keeps a
   re-push from minting a duplicate every time.
2. **Otherwise mint a fresh uid** from the controller's injected entropy,
   serialized exactly the way `package_manifest::ensure_uid` stamps one
   (`ProjectManifest::write_json`).
3. **Write the stamped `project.json` to the board** over the wire
   (`/projects/<runtime storage id>/project.json`). The server applies it
   as an ordinary `FsEvent` → `Project::refresh_artifacts` incremental
   apply: no unload, no reload, the runtime handle the editor holds stays
   good.
4. **Re-read what the board runs** (fresh revision + hash) and require the
   hash to equal the stamped set's; a mismatch is a warn line and no
   install.
5. Then as before: a re-stamp binds the existing package and records the
   association; a mint installs under the minted uid
   (`PulledFromDevice`), records the association, and binds.

The bind's association arm (D6's second candidate) now skips an
identity-free board: it is not a VERSION of the associated project
(identity is the uid, D17), so it goes to adoption, where step 1
recognises the copy. Without that, a re-pushed board would stop at D4's
"not running the library's copy" line and never be re-stamped.

**Still refused, unchanged:** no `project.json`, a `project.json` that
does not parse, and a uid the library holds at another version (D4).
Everything stays best-effort: any failure is one warn line, and the open
keeps working unbound. A failure after the write leaves the board
carrying a uid, which the next open binds or adopts the ordinary way.

**Why this does not break D17.** D17 forbids minting a second identity
for a project that already has one. An identity-free project has none;
stamping one — on the board, where the project actually lives — is
exactly what entering a library does to any other project. The rejected
alternative below (minting only in the library) stays rejected for the
reason it always was: the copy would never hash-match the board.

## Alternatives considered

**A confirmation dialog before adopting.** Rejected: nothing about
adoption is destructive or ambiguous enough to need one. It writes a new
package under a uid nothing else in the library holds, keyed to content
that is verified byte-for-byte against what the board reports. A prompt
here would just be friction in front of an operation that cannot lose
anything.

**Minting a fresh uid for the adopted copy.** Rejected: it would split
one project into two identities across libraries, which is exactly what
D17 rules out. (For an identity-free board, minting ONLY in the library
is also rejected — the copy's `project.json` would differ from the
board's and never hash-match it — which is why the amendment stamps the
uid onto the board instead.) The fresh-uid move belongs to a different situation
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
