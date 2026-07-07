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
  history/<prj_uid>/    lpc-history roots — beside, never inside, packages
```

## Who mounts it

The **studio main thread** (`lpa-studio-web::local_store`), at startup,
after taking the `lp-library` Web Lock (`acquire_exclusive_lock` — origin
wide, auto-released on tab death; a refused acquisition is the "open in
another tab" state). The **simulator never mounts this store**:
persistence belongs to the local project store, and the sim is an
ephemeral place — opening a project is a push, saving is a pull (roadmap
D19/D20; the wire transfer lands with milestone M2b).

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
