# lpa-firmware-store

Studio's engine cache and the firmware store client. Sans-IO, host + wasm:
no executor, no clock (time is the caller's f64 epoch seconds), no HTTP
client — the edges inject the storage and the fetch.

## The engine cache

Every engine Studio installs, fetches or reads back is kept by SHA-256 of
`engine.bin` as flashed — the same value as the core's digest slot, the
board's `engineSha256` and `ota-manifest.json`'s `engine.sha256` — so the
common case needs no network.

- **The seam:** `EngineCache` (`put`, `get`, `has`, `index`, `set_held`).
  `put` verifies the length and SHA-256, stores, **reads back equal**, then
  evicts; `get` re-verifies (a corrupt file is dropped and reported
  `Missing`) and marks the entry used.
- **The index:** `firmware-cache/index.json`, `{ "format": 1, "entries": [...] }`
  (`format`, never `version`: an entry carries an app `version`). Cache-like:
  unreadable or a foreign format reads as empty.
- **The policy:** bounded at 64 MiB, evicted **least recently used first**
  (the brief's "oldest first", read so an engine Studio keeps using stays),
  never a `held` entry (M7 pins the backup of an in-flight update) and never
  the one just put.
- **Backends:** OPFS in `lpa-studio-web` (`engine_cache_opfs.rs`);
  `MemoryEngineCache` for tests, sims and hosts without OPFS.
  `check_engine_cache_contract` is the contract both keep.

## The store client

`FirmwareStore` builds lookup URLs on an origin —
`DEFAULT_FIRMWARE_STORE_ORIGIN` (`https://lightplayer.app`) or Studio's
`?firmware-store=` dev flag — and verifies everything it fetches through an
injected `FirmwareFetch`: a manifest must validate and be the target and
release asked for; a file must have the manifest's length and SHA-256.

`fetch_engine_from_store(store, target, build_id, expected_engine_sha256)`
answers `Ok(None)` for a dev build **without fetching** (dev builds are never
in the store) or for a release the store lacks, and
`Err(StoreError::EngineMismatch)` when the store has the build but not the
engine the board reported.

## The engine a USB install wrote

`keep_installed_engine(fetch, cache, manifest_url, now)`: after a successful
USB install, Studio reads the package back from where its flasher read it
(the bundle's `firmware/<target>/manifest.json` and the merged image beside
it), and for a split package `InstalledPackage` checks the image against the
package, slices the engine out by the `split` block's flash offset, checks
it against the block's SHA-256, and the engine is put as `installed`. A
package with no `split` block keeps nothing. The caller runs it after the
install's outcome is reported and only logs the answer
(`firmware cache: kept engine <sha8> (installed, <buildId>)`, or a warn).

## What is not here

Ordering the sources (cache → store → read-back) and read-back itself are
the update protocol's (`lpa-update`, M4): it emits effects, and the edge
answers them with this crate. Neither crate depends on the other. The
manifest format and the URL grammar are `lpc-firmware-release`'s.

## Files

| File | Concept |
|---|---|
| `engine_cache.rs` | the `EngineCache` seam, its errors, the contract check |
| `engine_cache_entry.rs` | `EngineCacheEntry`, `EngineSource` |
| `engine_cache_index.rs` | `EngineCacheIndex`, parse, eviction |
| `memory_engine_cache.rs` | `MemoryEngineCache` |
| `firmware_fetch.rs` | the `FirmwareFetch` port |
| `firmware_store.rs` | `FirmwareStore`, `StoreError`, the default origin |
| `fetch_engine_from_store.rs` | an engine for a board, verified |
| `installed_engine.rs` | `InstalledPackage`, `engine_from_merged_image`: a split package's engine out of its merged image |
| `keep_installed_engine.rs` | `keep_installed_engine`, `KeptEngine`: the USB install's keep |
