---
status: fixed          # closed by the conformance test and the lint that rode its fix
found: 2026-10-10      # e2e (plan discovery for the wire push boundary; the test confirmed it)
fixed: this change     # PR #1130
area: lpa-server `access_guarded_fs.rs` × lpfs `LpFs` defaults; lp-tree-store `lp_fs_tree.rs` change log
class: stand-in-divergence
related:
  - docs/adr/2026-10-10-fs-push-boundary-and-deflated-writes.md
  - lp2025/2026-10-08-2339-wire-push-boundary-and-deflate
---
# LpFs wrappers silently took the trait's defaults, and an aborted batch kept its change log

**Symptom** — through `AccessGuardedFs` (the wrapper every loaded project's
filesystem wears), `begin_batch`, `commit_batch` and `abort_batch` answered
`Ok(())` without reaching the filesystem underneath: a batch a project began
through its own fs was a silent per-call commit, and an abort dropped
nothing. Nothing failed; the writes simply were not atomic. Separately, on
the tree store, an aborted batch left its writes and deletes in the RAM
change log, so `ChangesSince` after an abort reported a **Delete tombstone
for a file that still existed** (`file_sync` emits a delete without
re-reading the file) — a Studio pull after an aborted push would have been
told an existing file was gone.

**Root cause** — `LpFs` grew defaulted methods (`append_file`, `file_size`,
the batch trio) whose defaults are correct for a *backend* ("commit each
call by itself") and wrong for a *wrapper*, which must ask what it wraps.
`AccessGuardedFs` implemented the trait method by method and simply did not
name the batch trio, so the compiler filled in the backend defaults. A
default is a stand-in that diverges from the real answer exactly where
nothing tests it. `LpFsTree::abort_batch` called the store's abort and
nothing else; its change log is recorded per call, outside the store.
Neither was reachable from the wire until the wire could begin and abort a
batch (M6).

**Fix** — `AccessGuardedFs` and `LpFsView` forward every method, including
the new `batches_are_atomic` and `write_deflated_chunk`, and carry
`#[deny(clippy::missing_trait_methods)]`, so a future `LpFs` method fails
clippy until each implements it. `lpa-server/tests/lp_fs_wrapper_conformance.rs`
drives every method through each wrapper over a recording filesystem and
checks it was heard (it failed first on `AccessGuardedFs::begin_batch`,
"heard []"). `LpFsTree` snapshots its change log at `begin_batch` and puts it
back at `abort_batch` (`lp_fs_tree_tests.rs`
`an_aborted_batch_leaves_the_change_log_as_it_was`, and over the wire
`tests/batch_state.rs`).

**Lesson** — a trait default written for backends is a trap for wrappers.
Wrappers deny `missing_trait_methods`; a conformance test asks the wrapped
thing what it heard, for every method, not only the ones a change touched.
