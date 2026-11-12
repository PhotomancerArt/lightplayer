---
status: fixed
found: 2026-09-25        # how: report (planning discovery, multi-pattern projects)
fixed: this change
area: lpc-engine node/node_tree.rs (RuntimeNodeTree)
class: dense-over-monotonic-ids
related:
  - lp2025/2026-09-24-2351-multi-pattern-projects (plan PD3, phase P1)
---
# The node tree kept a tombstone for every removed node, so each reload grew it

**Symptom** — found in planning, before any cycle ran. Dormant playlist
entries remove the playing entry's subtree and attach the next one on
every pattern switch, so a cycle would grow the tree's slot storage by the
size of one subtree per switch, forever. The new test measures it on the
host: three nodes attached and removed 100 times after a warm-up retained
**145,152 B** more heap under the old storage, and **0 B** under the new
(`live heap after warm-up: 7549 B; after 100 more reloads: 152701 B`,
then `6397 B; after 100 more reloads: 6397 B (growth 0 B)`).

**Root cause** — `RuntimeNodeTree` stored `Vec<Option<RuntimeNodeEntry<N>>>`
indexed by `NodeId`. `remove_subtree` replaced a removed entry with a
`None` tombstone, `next_id` only increments (ids are deliberately never
reused, so a stale id can never alias a new node), and the vector never
shrank. Its length therefore tracked every id ever minted, not the nodes
alive. That was harmless while a project loaded once and edits were rare;
it stops being harmless the moment reloading a subtree is routine.

**Fix** — the entries live in `NodeEntrySlots`
(`lpc-engine/src/node/node_entry_slots.rs`): a `Vec` of live entries only,
sorted by id. A removed entry is dropped with its slot, storage tracks the
live count, and ids stay monotonic and unique. Because ids only grow, a new
node appends at the end and iteration stays in id order, as before. Lookup
stays O(1) in the common case: an entry's position never exceeds its id and
equals it until something below it is removed, so a lookup tries that
position first and binary-searches the prefix below it only on a miss. The
binding index's `rebuild` and `binding_by_ref` read the same storage.

A plain `lp_collection::VecMap<NodeId, _>` (binary search on every lookup)
was tried first and cost measurably more on the steady-render hot path, so
it was replaced. Emulated steady-render cycles
(`lp-cli profile --collect cpu --mode steady-render`, `fw-emu` on
`lp-riscv-emu`, `esp32c6` cycle model; the count is deterministic, a rerun of
the baseline reproduced it exactly):

| project | before | `VecMap` | `NodeEntrySlots` |
| --- | ---: | ---: | ---: |
| `projects/test/basic` | 4,840,263 | 4,846,515 (+0.13 %) | 4,840,375 (+0.002 %) |
| `catalog/projects/fyeah-sign` | 6,569,679 | 6,583,595 (+0.21 %) | 6,571,899 (+0.03 %) |

The remaining difference sits in the resolver's query interning, which this
change does not touch (`hash_slot_path` inlined in one build and not the
other); no node-tree function moved.

The engine's other per-id side stores were checked and are keyed, not
positional, and already forget removed ids: `TreeEntryStamps` and
`StateRootStamps` (pruned against the tree when a read refreshes them),
`ProjectRuntimeIndex` (`remove_runtime_node`, from
`Engine::remove_runtime_subtree`), the timebase store (swept each tick),
the engine's `output_identities` (pruned on register), and the resolver's
`static_paths` (cleared on invalidation).

**Regression coverage** — `lpc-engine/tests/node_tree_reload_memory.rs`
(`reloading_a_subtree_retains_no_heap`, counting allocator): fails on the
old storage with the growth above, passes on the new. The
`node_entry_slots` unit tests pin lookup after removals and that capacity
follows live entries, and `tree_next_id_never_reused` still pins id
uniqueness.

**Lesson** — dense storage indexed by a never-reused id is sized by how
many ids were ever minted, not by what is alive. It is correct and cheap
exactly as long as removal is rare, and it becomes a leak the day a
feature makes removal routine — with no code change at the storage site.
When a design turns a rare operation into a per-interaction one, audit
every structure whose size follows that operation's history.
