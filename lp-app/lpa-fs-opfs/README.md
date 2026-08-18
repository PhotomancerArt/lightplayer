# lpa-fs-opfs

The browser's **local project store**: OPFS-backed persistence behind the
sync `LpFs` trait, plus the Web-Locks single-writer guard. This is where
the library — package directories and their `lpc-history` areas — durably
lives on the web platform.

A **platform edge** crate per `docs/adr/2026-07-06-sans-io-core.md`: the
executor coupling (`wasm-bindgen-futures`, timers, `spawn_local`) lives
here so `lpfs` and the core stay executor-free. wasm-only — registered in
workspace `members` but not `default-members`.

## Design: memory-primary + async write-behind

`LpFs` is synchronous; OPFS is Promise-based, and wasm can't block on a
promise. So the store never tries: at `LpFsOpfs::mount` the whole OPFS
tree loads into an in-memory fs (KB scale, milliseconds), every sync
`LpFs` call hits memory unchanged, and a **driven** flusher
(`run_flush_loop`, spawned by the host) drains the fs change log
(`FsVersion` / `get_changes_since`) to OPFS about 100 ms behind.

Writes go through `createWritable`: staged, then swapped in atomically at
`close()`. A killed tab mid-flush leaves each file at its previous
version — stale by ≤ ~100 ms at worst, never torn. That is the durability
contract: *"saved within a blink," not "saved before the write returns."*
(A SharedArrayBuffer sync-bridge would close that window but requires
COOP/COEP headers, which plain static hosting can't provide — rejected;
see the ADR's alternatives.)

Two sharing subtleties encoded here:

- `LpFsOpfs` clones share all state, and `chroot` builds views **over the
  store itself** — `LpFsMemory::chroot` clones its change log rather than
  sharing it, which would hide view writes from the flusher.
- No `RefCell` borrow is ever held across an `await`; flushing snapshots
  dirty state synchronously, then does IO with no borrows outstanding.

## Layout on OPFS

```
<opfs root>/lightplayer-library/
  packages/<dir>/       package directories (projects, later modules)
  history/<prj-uid>/    lpc-history roots — beside, never inside, packages
```

## The locks (`library_locks`)

Two typed Web Locks, not one page-wide lock (that was M2, and it blocked
project Y in a second tab because project X was open in the first). See
`docs/adr/2026-07-08-per-project-library-locking.md`.

- **`lp-project:<uid>`** — exclusive, taken when a project opens and held
  while it stays open. Guards that project's `/packages/<slug>/**` and
  `/history/<uid>/**`; the holder being the only writer is what makes
  write-behind correct.
- **`lp-catalog`** — short-lived, guarding catalog *structure*: package
  directory create/remove/move, `/registry.json`, seed-once installs.
  Transactions flush fully before releasing.
- **Ordering: Project before Catalog, never the reverse.** Reads take no
  locks — gallery data is a fresh read-only mount, and whole-file atomic
  writes make torn files impossible.

Acquisition *policy* is the caller's, which is why this crate offers both
`try_acquire` (one `ifAvailable` shot — for a structural op the refusal
is the answer) and `try_acquire_polling` (a bounded ladder — for a caller
whose refusal is only ever momentary, like an open racing this tab's own
release).

**A lock is held for local OPFS work only, never across network IO**
(ADR amendment 2026-08-14). A caller whose work is mostly elsewhere —
cloud sync — mounts the project under the lock, releases, does the round
trip against that in-memory snapshot, and reacquires briefly to bank what
came back; `LpFsOpfs::pending_writes` exists for the case where the
write-back has to be handed to a store that took ownership meanwhile.
Holding longer would not just be slow: a project lock's refusal is the
"open in another tab" message, so a hold set by foreign latency makes the
UI say something untrue.

## Who mounts it

The **studio main thread** (`lpa-studio-web::library_host_opfs`), which
mounts per scope rather than whole-library: a fresh read-only tree per
catalog transaction and per gallery snapshot, and one memory-primary
store with a flusher per open project subtree (under that project's
lock). The **simulator never mounts this store**: persistence belongs to
the local project store, and the sim is an ephemeral place — opening a
project is a push, saving is a pull (roadmap D19/D20; the wire transfer
lands with milestone M2b).

## Tests

Real-browser tests over real OPFS (`wasm_bindgen_test`, `run_in_browser`):

```bash
just lpa-fs-opfs-test
# or directly:
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
    cargo test -p lpa-fs-opfs --target wasm32-unknown-unknown
```

Requires `wasm-bindgen-cli` (matching the workspace `wasm-bindgen`
version) and a `chromedriver` matching the local Chrome major version —
set `CHROMEDRIVER=/path/to/chromedriver` if the one on `PATH` mismatches.
Coverage includes the two-mount reload round-trip, flush coalescing and
watermark honesty, chroot-view change capture, lock refusal semantics,
and `lpc-history` running end-to-end over the store.
