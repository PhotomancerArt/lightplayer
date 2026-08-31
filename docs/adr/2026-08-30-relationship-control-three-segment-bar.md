# ADR: The relationship control — a three-segment session bar with ownership, changes, and history

- **Status:** Accepted
- **Date:** 2026-08-30
- **Deciders:** Photomancer
- **Plan:** lp2025/2026-08-30-0204-project-relationship-control (PR #483)
- **Supersedes:** None
- **Amends:** `2026-08-19-single-session-web-and-session-control.md` (the
  fused-lockup rationale: segments open different panels now) and
  `2026-08-28-viewing-is-stateless-fork-at-explicit-save.md` (fork is
  still the explicit save; it is no longer gated on being dirty)
- **Superseded by:** None

## Context

The app had no notion of "my relationship to this project," so every
affordance that depends on one invented its own answer — and several
answered by disappearing.

- The **"example" pill** in the header lockup was a plain `span`, dead by
  architectural constraint: it lived inside a `DetailPopover` trigger
  subtree, which renders twice while open and must stay stateless.
- The **Share pill** was the good sharing surface (URL hero, access
  segment, people list) but self-gated on `members.is_some()` from one
  `GetProject` — so it drew only for the owner/editor of a *published*
  project. Anonymous, unpublished, no-saved-head, example, and visitor
  sessions got no pill at all: it vanished exactly when standing was most
  in question.
- The **old share rows** (copy link / zip / JSON) still sat in the crowded
  project detail popup, a previous generation of the same idea.
- **Fork lived in four places**: the visitor banner, the visitor share
  popover, the gallery's Duplicate, and the invisible fork-at-save.
- **The driving pain**: open an example, decide to keep it *without
  editing first* — impossible. Save exists only while the overlay is dirty
  (the presence of `header_actions` IS the dirty test), the example pill
  was dead, the Share pill was gated out. A pristine transient session had
  zero affordances.
- **Ownership was triangulated, never modeled**: `ShareMode` (derived from
  whether the server answered a roster), `TransientOrigin`, and
  `library_identity`. `ProjectMeta.owner` came back from the service and
  the UI never read it.
- **History had an engine and no face**: `ProjectHistory::events()` had
  zero non-test UI consumers.

Direction converged in the UX spike `spikes/relationship-control/`
(six committed gate rounds on this branch, `dcda852b0` at convergence).
The spike is the design record and is visual reference only — production
code never imports from `spikes/`. This ADR records the shipped decisions;
the vision's D1–D13 and the round-by-round rulings live in the planning
archive.

## Decision

### 1. One derived relationship, rendered everywhere

`ProjectRelationship` (`app/share/relationship.rs`) is a five-state enum —
`Example`, `ViewingSomeoneElses`, `MineLocal`, `MinePublished`,
`MemberOfSomeoneElses` — **derived, never stored**. `derive_relationship`
is a pure function over values the app already had, plus one it never
read:

- `open_project_transient` / `open_transient_example` (the core view's
  transient pair): a transient session is an example or someone else's
  shared project, and nothing else matters once that is true;
- `in_library` (`library_identity.is_some()`): the precondition for every
  owned state;
- `roster_answered` (`members.is_some()` from one `GetProject`): the
  service saying this viewer is on the roster of a published project;
- `owner` vs `viewer` — **the first UI read of `ProjectMeta.owner`**,
  compared against the session's own actor (`Actor::User(uid)` signed in,
  `Actor::Anonymous` otherwise).

**The ambiguous-owner fallback is "mine", and it is deliberate.** When the
comparison cannot be made — either side unknown while a fetch is in
flight, or both sides `Actor::Anonymous` (two guests are
indistinguishable identities in an API with no display names) — the
derivation returns `MinePublished` rather than guessing membership. It is
the honest merge for v1: the states it can distinguish it distinguishes
exactly, and the one it cannot it names in the code rather than papering
over with a heuristic. Owner attribution by *name* needs server work
(`Actor` carries no profile); the faces are designed so a name drops into
the word's position unchanged.

The face is **neutral** (no accent, post-`#478`; no status-blue for
"Shared"): a face states who this document is to you, which is identity,
not health.

### 2. The bar is three segments — tabs into ONE shared panel

`[ device | project | changes ]` — one bordered shell with internal
hairlines, and Save standing beside the shell as the one direct act.

Round 1 shipped this as three independent `DetailPopover` triggers; the
G1 round-2 ruling (D15) replaced them with **one panel, segments as
tabs**: moving between sections was a close-reopen-animate cycle for
what reads as switching tabs on a single control. The segments are plain
buttons now, driving a lifted `(open, section)` pair; the shared panel is
one `DetailPopover` whose own trigger is hidden, whose **controlled
open-state** (`open_signal`, added to the popover primitive for this)
the segments write, and whose merged outline anchors on the WHOLE shell —
the panel visibly hangs off the bar, and the bar is the tab row. Clicking
another segment while open switches the content **in place** (the panel
ResizeObserver and outline retarget absorb the resize); clicking the open
segment closes. While open, the segments render again in the top layer as
the popover's interactive anchor visual, so the tabs keep working above
the merged outline. The panel width is pinned to the 320px detail card
across sections so switching never jiggles.

**This amends `2026-08-19-single-session-web-and-session-control.md`.**
That ADR ruled (R8-2) that *every segment of the lockup opens the panel*,
and the shipped control washed on hover as ONE object — correctly, given
its own premise: device and project did the same thing, so lighting them
separately would have promised a distinction that did not exist. **The
argument inverts once the segments open different content.** Each segment
now answers a different question — what is running · what document is open
· what is in flight — so the hover wash rides the segment under the cursor
and the shell stays quiet, and the open segment wears the selected
treatment as the panel's active tab. The wash is now the honest promise it once
would have faked. Everything else that ADR decided stands: one session per
tab, navigation is studio-or-site, the ungated single mount, and Save as a
sibling button rather than a segment (the stateless-trigger constraint is
what makes the sibling necessary, and it is unchanged).

Three questions was also the answer to *where the old detail popup goes*:
the device section states what is running, the changes section lists what
is in flight over the banked history, and the project section is the
document's own panel, with the surviving settings/identity/stats sections
behind its ⋯ menu's Details row.

### 3. Changes are their own concept, and Save stays a sibling

Pending edits span BOTH stores: they are already live on the device the
moment you touch a control, and Save banks them to the library and the
cloud — "save it everywhere". They belong to neither the device box nor
the document box alone, so they get the third segment.

The changes popup owns the edit list, per-entry revert, revert-all, and
the save receipt; the ↺ sibling retires into it. **The segment is always
present** — a quiet ✓ when clean, the count when dirty — so "nothing
pending" is a state you can click into and confirm rather than an absence
you must infer.

**Save stays a sibling button**, and the controller's contract is
untouched: `ProjectEditorView.header_actions` is still published, still
present only while persisted edits are pending (presence-is-dirty), and
the bar only re-homes the rendering. The one place that could not use it —
a pristine transient session's "Save a copy", where `header_actions` is
empty exactly when the button must work — dispatches `ProjectOp::SaveOverlay`
directly rather than minting a synthetic pane action.

### 4. One popover skeleton for all five states; Where owns the URL

The project section renders the same skeleton in the same order for every
relationship: identity block → **Where** → **Access** → a fixed action
row. Only the words and which controls are live change. (Round 1 carried
an inner `[Project | History]` tab row; D14 deleted it — see §5.)
Five panels that each invented their own shape is what the spike's
rejected rounds looked like.

- **Where** states your standing and carries **the URL**. The address bar
  IS the share link (`2026-08-08-project-url-identity-and-sharing.md`
  D1/D13), so the link is not a footer button and not an access control —
  it is *where this document lives*, stated in the section that answers
  "where am I". For an unpublished project this is also where the
  auto-publish ledger finally surfaces as product ("publishes on save
  while you're signed in"), completing the posture `#465` shipped as
  diagnostic-only.
- **Access** is purely who-can-do-what: the shipped segment + description
  + people list where the service answered a roster you administer, a
  read-only roster where you are on one you do not, and one honest
  sentence where there is nothing to administer. Never dead controls.
- **The action row's slot 1 is always a fork-family verb** — there is
  always a way to end up with your own copy — hero-tinted only where
  forking is *the* move. The verb is owned vocabulary: "Save a copy" for
  an example, "Fork — make it yours" for a visit, "Duplicate" for a
  project already yours (the gallery's own word, so one gesture has one
  name). Slots 2 and 3 are Copy link and the ⋯ overflow (zip, JSON,
  Details).
- Section heads are a small-caps label with a hairline rule to the panel's
  edge. The node-style vertical rail was built and **rejected** at spike
  round 6.

### 5. History is the changes panel's banked timeline (D14)

`ProjectHistory::events()` gets its first UI consumer as the read-only
**banked timeline under the changes section's pending block**: version,
kind, what, when — newest first, from core's capped projection of the
OPEN handle's own events. Round 1 put it behind a History tab of the
document popover; the G1 round-2 ruling moved it: **changes and history
are one temporal axis** — the receipt "Save banks v13" and the timeline's
"v12 saved" are the same ledger read from opposite ends — so the pending
block sits on top and the banked rows below, in one panel, with no tab
between them. The project popover keeps the identity axis alone. The
receipt now names the version Save will bank when the projection knows
it, and the timeline carries no synthetic "editing" row (the pending
block above IS the in-flight statement). No server fetch, so the timeline
never claims completeness it does not have, and a transient example gets
the honest empty state ("history begins at your first save") rather than
a list of the open's own bookkeeping.

**Restore stays parked.** `SnapshotStore::materialize` and
`LocalProject::checkout` exist; putting a verb on these rows is its own
effort behind this designed seam, and the footer says so plainly.

### 6. Fork is still the explicit save — now available pristine

**This refines `2026-08-28-viewing-is-stateless-fork-at-explicit-save.md`
without contradicting it.** That ADR's D7 ruled the explicit save gesture
IS the fork: a transient session's save commits, then installs into the
library (an example's uid promotes; a shared view mints a fresh identity).
Every word of that mechanism is unchanged. What changes is the *gate*: the
gesture was only reachable through the Save button, which exists only
while the overlay is dirty — so keeping an example required editing it
first, which is precisely the "viewing creates nothing" model biting the
person who decided to stop viewing. "Save a copy" is now available
whenever the session is transient, dirty or not; a pristine save commits
nothing (`written == 0`) and still runs the fork, so the same code path
serves both. Fork-on-first-*edit* remains rejected, for the same reason it
was rejected there.

### 7. The runtime popover is the device panel's declared landing zone

The device segment's popover states what is running: kind glyph, name, run
word, the simulating-board line, and the "this tab is the session" hint.
It is declared here as the **landing zone for the desktop device panel's
facts** when that panel retires (it duplicates the main visualization).
The retirement itself is a follow-on effort, not this one — but naming the
destination now is what keeps the next person from inventing a third home.

## Consequences

- **Retired**: the accent "example" pill; the ↺ sibling; the standalone
  Share pill (`ProjectShareControl` / `SharePillPopover` / `SharePanel`)
  and the ⋯ menu's "Sharing & access…" row; `ProjectShareSection` (the
  detail popup's Share block); the visitor's variant of the same pill slot
  (`VisitorSharePopover` / `VisitorShareSlot`); the visitor banner's
  pristine Fork CTA.
- **Nothing went homeless.** Every row the old `ProjectDetailSections`
  rendered has a named home: identity / settings / stats stayed (reached
  through the popover's ⋯ → Details, and still the pane's own [i] on
  non-workbench mounts); the pending-edit facts, the unsaved list with its
  per-entry revert, and the failed list moved to the changes popup; copy
  link became a fixed action-row slot; zip and JSON became the ⋯
  overflow's two rows, carrying their dirty-disable rule and its
  explanatory line verbatim.
- **The sharing controls are pure pieces now** (`app/share/access_controls.rs`):
  URL hero, access segment, description, people list, add row. They have
  one consumer — the relationship panel — and no popover of their own.
- **One `GetProject` per project route.** The pill and the popover both
  mounted the roster hook during the overlap phase; with the pill gone,
  `web_app` holds the single mount and its answer feeds both the
  derivation and the panel.
- **The visitor banner shrank to a status strip** (vision Q4: shrink
  first, retire later) — it keeps the pristine/edited/edit-live line,
  Copy link, and Discard. It kept exactly one fork: the **edited** state's,
  which is reachable only on a persistent tracking copy (an Edit link, or a
  pre-examples-vision View copy under the Q4 leave-alone ruling). Those
  sessions derive as `MineLocal`, so the popover offers them Duplicate
  rather than the tracking-copy fork with its provenance; removing the
  strip's button would have taken the only affordance that names what
  happened to them. See Follow-ups.
- **The example pill's disappearance is now a state change, not a
  vanishing act**: the bar's face flips from Example to Private when a
  transient session forks, which is the same moment the URL heals and the
  fork toast fires.
- The failed-only asset-edit blindness recorded by the session-control ADR
  is unchanged and still tracked as debt
  (`docs/debt/failed-only-asset-edit-header-blindness.md`); the changes
  segment now shows the failed count, but the controller still publishes no
  Save while `persisted == 0`.

## Alternatives Considered

- **A "third segment" that is really a sibling button** (the vision's
  original D3, honoring the stateless-trigger constraint by faking the
  fusion). Superseded by the spike: if the whole bar is per-segment
  clickable there is nothing to fake, and the popovers are panel content,
  not trigger content, so the constraint is satisfied honestly.
- **Keeping sharing on a pill in the right cluster.** Rejected: the pill
  self-gated into invisibility for four of the five relationships, and a
  door that disappears when standing is unclear is worse than no door.
  Placing it beside the project's own name is also just where people look.
- **Changes inside the project popover.** Rejected (D8): pending edits are
  not a property of the document's identity, they span the device and the
  library, and burying them under "what is this project" is the wart the
  old detail popup already had.
- **History as its own door / in-panel navigation.** Rejected round 1 in
  favor of a tab; the tab itself was then rejected at G1 round 2 (D14) in
  favor of riding the changes panel — same ledger, one panel.
- **Three independent per-segment popovers** (round 1 as shipped).
  Rejected at G1 round 2 (D15): section-switching read as tabs but cost a
  close-reopen animation each time; the lifted-state single panel switches
  in place.
- **A vertical section rail in the popover** (the node-tree idiom).
  Built at spike round 5, rejected at round 6: plain horizontal headings
  with a rule read better at this width.
- **Retiring the visitor banner entirely.** Deferred (Q4): its
  ahead/behind status is not duplicated anywhere, and a strip that says
  "updates are paused" earns its row.

## Follow-ups

- **Tracking copies derive as `MineLocal`.** An Edit-link visitor or a
  legacy View tracking copy is in the library with no roster answer, so
  the bar says "Private" and the popover offers Duplicate. The honest fix
  is teaching the derivation about tracking copies (the `VisitorSession`
  already knows); **revisit with** the visitor banner's own retirement.
- **Owner display names.** `Actor` carries no profile, so no surface names
  an owner. **Revisit when** accounts have profiles.
- **History restore/checkout**, behind the tab's declared seam.
- **The desktop device panel's retirement** into the runtime popover
  (D13).
- **Sweep-time healing of no-saved-head projects** — unchanged from `#465`;
  the state is now visible as product, which was the precondition.
- **Provenance prose** ("Forked from Plasma Duo") is not reachable from an
  open project: it lives on `PackageMeta` and surfaces only on gallery
  cards. The Where section says what this surface actually knows;
  **revisit when** the editor view carries provenance.
