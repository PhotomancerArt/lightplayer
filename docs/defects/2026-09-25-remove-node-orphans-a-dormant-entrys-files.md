---
status: fixed
found: 2026-09-25        # how: director review, multi-pattern projects P8
fixed: this change
area: lpc-registry node_authoring.rs (remove_node)
class: assumed-context
related:
  - lp2025/2026-09-24-2351-multi-pattern-projects (plan phase P8)
  - 2026-09-25-panel-restore-drops-dormant-entry-knobs.md
---
# `remove_node` left a dormant entry's files on disk

**Symptom** — found in review, before it shipped anywhere. `remove_node`'s
own doc comment claims it stages a delete for "everything that left the
effective inventory... exclusively referenced through the removed entry".
That was true only while a playlist's whole authored subtree was always in
the effective inventory. Once dormancy (P2) means derivation stops at a
non-resident entry, removing a dormant entry — or removing its whole
playlist while a sibling entry was dormant — deleted the entry from
`playlist.json` but left its def file and exclusively-referenced assets on
disk forever: they were never in the effective inventory to begin with, so
they could never appear in `defs.removed`/`assets.removed`.

**Root cause** — `staged_deletes` is computed as a diff of the *effective*
inventory before and after staging the removal's slot edit. A dormant
entry's def and assets are, by design (PD1), never part of that inventory,
so removing them left no trace in the diff for the removal to sweep.

**Fix** — `remove_node` widens residency to every entry, everywhere,
before computing the removal diff (reusing
`ProjectRegistry::make_every_entry_resident`), so a removed subtree's
dormant files are present to be swept exactly like a loaded one's. It then
narrows residency back to exactly what it was before the call — never
permanently, and never visible in the returned `RemoveNodeOutcome.changes`,
which is still measured against the pre-widen inventory (an unrelated
dormant entry elsewhere nets out to no change at all). An entry that fails
to load still gets a `NodeDefState`/`AssetState` error row rather than
vanishing from the inventory, so it participates in the same sweep as a
loaded one — removal never fails, and never silently skips a file, because
an entry is broken.

**Regression coverage** — `lpc-registry` `src/registry/node_authoring.rs`
unit tests: `remove_playlist_entry_removes_whole_entry_and_leaves_siblings`
no longer pre-loads the dormant entry it removes (the manual
`make_resident` call it used to need is gone), and the new
`remove_whole_playlist_sweeps_a_dormant_entrys_files_too` removes a whole
playlist with one resident and one dormant entry and asserts both entries'
files are staged for deletion.

**Lesson** — the same class as the panel-restore defect right before this
one: dormancy makes "in the effective inventory" and "authored" different
facts, and any operation whose diffing (or keying) assumed they were the
same question needs a second look once a parent can keep children out of
the tree.
