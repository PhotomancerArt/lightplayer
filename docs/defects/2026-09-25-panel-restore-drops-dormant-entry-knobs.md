---
status: fixed
found: 2026-09-25        # how: report (planning discovery, multi-pattern projects)
fixed: this change
area: lpa-server panel_state.rs (restore, snapshot) × lpc-engine PanelWriterStore
class: assumed-context
related:
  - lp2025/2026-09-24-2351-multi-pattern-projects (vision D12, plan AC6, phase P3)
  - 2026-09-25-node-tree-tombstones-grow-per-reload.md
---
# Panel restore dropped every dormant playlist entry's knobs

**Symptom** — found in planning, before dormant entries shipped. A playlist
now keeps only its playing entry loaded. At boot, every other entry is
absent from the tree, so `panel_state::restore` dropped each persisted
writer for it as "unknown scope". A reboot would forget the knob values of
every pattern that was not playing. Then P3's check on the catalog's real
`pulse` pattern module found a second way to lose them, without a reboot: a
pattern module's knob does not land in the entry's sink scope. Its shader
reads `bus:speed` from the scope it inhabits, the MODULE's scope,
`ScopeRef::Module { owner }`, keyed by the module's runtime id. Unloading
the entry and loading it again mints a new id, which orphaned the writer in
memory.

**Root cause** — restore decided whether a persisted scope still exists
from the live tree alone (`tree().scopes()`, the scopes some live node
inhabits). That was the same question until the tree stopped holding the
whole project: an authored, dormant entry is part of the project but has
no node. And the in-memory writer store keys a scope by its owner's
`NodeId`, which is stable across edits (reattach replaces payloads, never
entries) but not across an unload and a reload.

**Fix** — restore also accepts the sink scope of every *authored* entry of
each live playlist, read from the playlist's def. A writer for a scope
owned by a node inside a dormant authored entry is **parked** by its
persist path (`PanelWriterStore::park`). On unload, the residency step
parks the writers of every scope owned by a node in the leaving subtree.
On load, it re-engages them on the new ids by persist path
(`Engine::apply_residency`, `lpc-engine/src/engine/entry_residency.rs`).
`snapshot` writes parked writers too, so a snapshot taken while an entry
is dormant keeps its knobs. The file format is unchanged: it already keyed
by persist path. Writers for entries the playlist no longer authors are
still dropped.

**Regression coverage** — `lpa-server` `tests/panel_dormant_entries.rs`:
`a_dormant_entrys_knobs_survive_a_reboot_and_come_back_with_the_entry`
(a server-ticked switch, knobs in both scopes, the file, a reboot, a
snapshot while dormant, the reload) and
`a_writer_for_an_entry_the_playlist_no_longer_authors_is_dropped`.
`lpc-engine` `tests/entry_residency.rs`:
`knob_values_survive_unload_and_reload`, and `panel_writers.rs` unit tests.

**Lesson** — "a scope no live node inhabits" was a fine proxy for "a scope
the project no longer has" only while the tree held the whole project.
Once a parent can keep children out of the tree, "is authored" and "is
loaded" are different facts, and anything keyed by runtime id inside a
dormant subtree needs a stable key before it leaves.
