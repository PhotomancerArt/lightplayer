# ADR: Node authoring operations — dedicated create/remove, commit/stage asymmetry

- **Status:** Accepted
- **Date:** 2026-07-27
- **Deciders:** Photomancer
- **Supersedes:** The "creation happens via examples; the gallery has no
  create op" clause (D17) of the project-management roadmap decisions.
  Builds on the editing overlay model
  (`2026-07-04-studio-editing-model.md`), the pane grammar
  (`2026-07-05-studio-pane-grammar.md`), wire hello versioning
  (`2026-07-14-wire-hello-versioning.md`), and node card faces
  (`2026-07-26-node-card-faces.md`).
- **Superseded by:** None

## Context

Until now LP Studio could edit nodes but not create or remove them. The
engine and registry already handled node add/remove driven by file
changes (`lpc-registry/tests/project_change_sets.rs` proves incremental
discovery and teardown); what was missing was a sanctioned way for a
client to cause those file changes. The `nodes` map on `ProjectDef`
carries `#[slot(policy = "read_only_persisted")]` with a comment
promising "dedicated project operations" — a policy that actually landed
as presentation polish (`a48666017`, 2026-07-06) to keep newly visible
root rows from offering edit affordances, with the ops comment as
forward-looking justification and no ADR. Playlist `entries` stayed
raw-slot-writable by omission.

Discovery (2026-07-27) established three hard constraints:

- **`ArtifactOverlay` is Slot-XOR-Asset**
  (`lpc-model/src/project/overlay/artifact_overlay.rs`): `put_slot_edit`
  on an artifact with a staged body silently discards the body. "Create
  staged in the overlay, then edit before Save" is broken at the
  data-model level — the first slot edit on the new node would destroy
  it. A staged-only node would also vanish on page reload.
- **Orphaned overlays abort Save**: nothing swept overlay entries when
  an artifact left the effective inventory; `commit_overlay` errors
  `CommitError::Projection` on a slot overlay with no effective def,
  aborting mid-write (commit is non-transactional).
- **Pre-commit delete needs two overlay pieces**: an
  `AssetBodyOverlay::Delete` on a def leaves an errored node in the
  tree; only a `Remove` slot edit on the referencing entry triggers
  `uses.removed` → runtime teardown.

## Decision

### Dedicated `CreateNode` / `RemoveNode` project commands

Node lifecycle goes through two new wire commands implemented in
`lpc-registry` core (`registry/node_authoring.rs`), inherited unchanged
by every server host including device firmware. The `nodes` map keeps
its `read_only_persisted` policy: generic slot gestures stay suppressed,
and the dedicated ops bypass the policy internally — they are the
sanctioned path the original policy comment promised. The historical
asymmetry is now intentional: playlist entry *fields* remain
slot-editable; node *lifecycle* (attach/detach, file create/delete) goes
through the ops at both sites.

`CreateNode { file, body, assets, attach }` takes **bytes**, not a kind:
the def JSON and sibling assets are authored client-side (see
templates). Validation is all-before-write (parse the body, reject kind
`Project`, reject occupied keys/paths — including occupied only in the
overlay); the attach rewrites the **base** file through the canonical
kind-first writer so pending overlay edits on the attach artifact ride
above it rather than being baked to disk. `RemoveNode { site }` stages
the site `Remove`, `Delete` on the def and every asset exclusively
referenced by the removed subtree, computed by inventory diff (shared
artifacts survive by construction). Both ops re-derive and live-apply to
the running engine through the same change-summary path fs events use.
One `WIRE_PROTO_VERSION` bump (1 → 2) covers the pair.

### Commit-on-create, staged-remove — deliberately asymmetric

**Creation commits immediately** to the session filesystem. The
Slot-XOR-Asset trap and reload-unsafety rule out overlay-staged
creation; restructuring the overlay to compose body+slots was rejected
as a far larger change for a marginal benefit. The recovery story is
history, not revert: after a create the Studio client runs the existing
save-pull, so creation lands in the library as a `Saved` event ("we have
a history. its fine." — Yona, 2026-07-27).

**Removal stages in the overlay**: destructive actions deserve
revert-until-Save, and the overlay's existing delete vocabulary covers
it. The op additionally performs a **recursive overlay sweep** of the
removed subtree (pending slot/asset edits on the removed node and, for
containers, every descendant) because orphaned overlays abort commit
mid-write. Sweep is realized as overlay-entry replacement, so reverting
a staged removal needs only existing ops (`RemoveSlotEdit` at the site +
`ClearArtifact` per staged delete); swept edits are not restored —
accepted, and the confirmation dialog warns. Removal does not cascade:
dangling refs keep erroring visibly on dependents (QC6), and the
client-side pre-flight counts dependents for the confirmation.

### Attachment site as the seam

`NodeAttachSite` (`lpc-model`) addresses either the project `nodes` map
(`ProjectNodes { key }` — the policy-locked site, op-bypass only) or any
writable `NodeInvocationSlot` path (`Slot { artifact, path }` — playlist
entries today). One create flow, two attachment sites; the byte-oriented
payload means future sources — duplicate, import, example gallery —
reuse the op and the picker UI unchanged. v1 builds only the blank
source; the source dimension is a designed seam, not built.

### Auto-naming, no manual names

Create is pick-kind → done. Names are kind slugs deduped `_2`/`_3`
(underscores: node names are `nodes` keys and tree segments, and
`NodeName` rejects hyphens), unique against both effective map keys and
project files. No name field anywhere; rename is the top follow-up.

### One starter-template module, all kinds instantiable

The picker enumerates every `NodeKind` except `Project` with **zero
per-kind create-flow code**: `starter_for_kind` (`lpc-model`
`nodes/starter.rs`) is a pure data table over
`NodeDef::default_for_kind`, supplying overrides only where the bare
default is not a usable authoring target (Texture 64×64, Shader
red-pulse `.glsl` scaffold + `time` slot, ComputeShader scaffold,
Fixture ring mapping). Kinds without an entry use the bare default even
if inert in sim (Button/Radio are device-authoring targets). Adding a
node kind must never require touching the create flow. The previous
three hand-maintained project templates collapsed into this module plus
one shared starter-project composition (`lp-cli` reuses it; dead
`lpa-server/src/template.rs` deleted).

### Gallery gains `CreateProject` (D17 deviation)

The gallery's "New" chip creates a **pure blank** project — one
`project.json`, black canvas — via `HomeOp::CreateProject` →
`CatalogOp::Create` → open. This supersedes D17's "creation only via
examples": the examples place (M6) is unbuilt, example seeding is
seed-once (wrong for repeated creation), and node authoring makes an
empty project genuinely useful. Examples remain the starter-with-content
path. En route this fixed a latent contract bug: `LibraryStore::create`
wrote a manifest without `format`, so a Created package was unloadable
(never hit because create was unreachable from UI).

## Consequences

- Node create/remove works against any server host. **Device sessions
  commit creation to the device fs immediately; the library `Saved`
  history safety net exists only for library-backed sessions.** Accepted.
- Tree deltas ride `ProjectRead`, not the mutation ack, so the Studio
  client triggers an immediate refresh after either op and re-reads the
  inventory after create (def-artifact map staleness otherwise makes the
  new node uneditable until the passive cadence).
- The save panel renders staged removals as first-class rows
  (`UiPendingEditKind::NodeRemoved` + deleted-file rows); after a
  reconnect the surviving server-side overlay degrades to plain
  removed/asset rows — node labels are not recoverable post-removal.
- The always-present project-pane "+" changed `project_header_actions`
  from dirty-gated to always-on; the web renderer intercepts it to open
  the kind picker.
- `MutationRejectionReason` gained `InvalidBody` and `InvalidPath` for
  create-request validation.
- Removing a node whose def is referenced by another surviving node
  detaches only the removed site's entry; the def survives (shared).
- Three consequences surfaced by the live walk and folded in: playlist
  entry keys gap-fill from **1**, not 0 (the bare `idle_entry: 1`
  default and the shipped examples are 1-based, so the first added
  visual lands on the idle key and plays immediately); an **empty**
  playlist entries map derives an empty-strip face rather than the
  generic fallback (a fresh playlist's card must carry the strip's add
  affordance); and both op acks re-read the def-artifact map (the
  engine may rebuild a site's runtime node under a fresh id when its
  def changes — a stale map orphans the staged rows).

## Alternatives Considered

- **Composed raw slot edits** (the archived M2 brief's assumption):
  rejected — batches are per-command and non-transactional, so a
  composed create can partially succeed (map entry without file = error
  node); the policy would also have to be dropped, reopening generic map
  gestures the UI deliberately suppresses.
- **Relaxing `read_only_persisted` instead of op-bypass**: rejected —
  the lock is presentation polish worth keeping (no raw add/remove/move
  gestures on the root map), and dedicated ops give atomic validation.
- **Overlay-staged creation (QC7's original aim)**: rejected on the
  Slot-XOR-Asset trap and reload-unsafety; restructuring
  `ArtifactOverlay` to compose body+slots was out of proportion.
- **Commit tolerating orphaned overlays** instead of the removal sweep:
  rejected — silently dropping unrelated orphans at Save hides bugs; the
  sweep keeps the invariant "everything staged is committable".
- **Manual naming at create**: rejected by Yona — name prompts only
  when really needed; auto-names + (future) rename cover it.
- **Per-kind create code paths / curated kind subset**: rejected — the
  all-kinds constraint keeps the flow generic and forces per-kind polish
  into data.

## Follow-ups

- **Rename node** — top follow-up (auto-names are the only names);
  needs a `MoveSlotEntry` unblock or dedicated op plus
  `node:`/relative-ref rewrite.
- **Duplicate node** — same op, bytes from an existing node; pre-builds
  import mechanics.
- **Import nodes from examples/projects** — own UX spike; the
  bytes+attach seam and the picker's source dimension are the landing
  zone.
- Dangling-binding cleanup assist; registry-side defaults from slot
  shapes; nested sub-projects; picker stays open after create (P5
  refinement candidate for the visual gate).
