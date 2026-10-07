---
status: paying-down
since: 2026-07-28      # sccache on the GHA backend made the budget a shared resource
logged: 2026-09-25
area: CI / GitHub Actions cache
related:
  - story-capture-pipeline.md
  - ../../.github/actions/cargo-registry/action.yml
  - ../../.github/actions/xtensa-toolchain/action.yml
---
# The repo's 10 GB Actions cache is over-subscribed

**Shape** — every cache in CI shares one 10 GB, LRU-evicted budget per
repository: sccache's objects (thousands of small entries), the cargo
registry, the stories job's tool+target bundle, Chrome for Testing and the
emu-ref reference builds. Nothing enforces a per-purpose quota, and GitHub
evicts by *last access*, so the effective horizon is "how long until the
repo writes another ~10 GB". A writer that saves often — or saves many
copies of the same bytes — shortens that horizon for everyone, and the
first victims are the entries read least often: exactly the ones that are
expensive to rebuild (emu-ref: an 8-minute pinned lp-cli build; stories:
620 s of `cargo install`). It is structural because every new cache is
added by someone looking at their own job's clock, not the budget.

**Carrying cost** — caches that exist on paper but miss in practice. The
emu-ref cache (#797, key `emu-ref-c6-…`, 26 MB) was evicted within about an
hour of being written; the stories entry was missing on 2 of 3 main runs on
2026-09-08; the Xtensa toolchain cache (#600) was gone within 25 minutes and
was removed for it. Each miss is minutes of CI on the critical path, and the
evictions are silent: the job just runs cold.

**Workarounds** — measure before adding anything over ~100 MB:

```bash
gh api repos/PhotomancerArt/lightplayer/actions/cache/usage
# the listing API returns at most ~7000 rows per ordering; read both ends:
gh api --paginate "repos/PhotomancerArt/lightplayer/actions/caches?per_page=100&sort=last_accessed_at&direction=asc" \
  --jq '.actions_caches[] | [.id,.key,.ref,.size_in_bytes,.created_at,.last_accessed_at] | @tsv'
```

The number to watch is the **oldest `last_accessed_at`** — that is the
eviction horizon. Anything read less often than that is not really cached.
The rules pre-merge.yml's budget note states: save from main only, PRs
restore main's entries; one entry per purpose.

**Incident log**

- **2026-09-08** — 11.7 GB in 5,597 entries: 5.8 GB was twelve per-job
  rust-cache copies of the same cargo registry. Collapsed to one per
  toolchain family (x64-nightly, xtensa-fw, xtensa-host, emu-esp32v3,
  emu-esp32s3, arm64-nightly, deploy); rust-cache saves made main-only.
- **2026-09-25** — 10.86 GB visible (11.54 GB reported by `cache/usage`,
  7,988 entries), and the **oldest last-access in the repo was 13 minutes
  old**. The emu-ref, stories and Chrome entries were all absent. Measured
  breakdown (02:15 UTC):

  | what | entries | size | written from |
  |---|---:|---:|---|
  | sccache objects | 5,722 | 4.79 GB | main |
  | sccache objects | 1,352 | 1.41 GB | PR refs — 1.40 GB of it from PR #811 alone, in two hours |
  | rust-cache registry, 6 families × 1-2 generations | 9 | 4.66 GB | main |

  The family fix of 09-08 was not enough, for two reasons. (1) rust-cache
  hashes every Cargo.toml into its key, so any manifest edit reaching main
  re-saved *all* families at once: the 17:00 and 02:06 main runs each wrote
  ~2.07 GB of rust-cache (4 × 517 MB) plus the arm64 one at 15:05 — the
  same registry bytes, keyed apart only by toolchain env hash. (2) sccache
  wrote on PR refs, whose entries only that PR can read, yet they count
  against the shared budget. Paydown in the same change:
  - ONE cargo-registry entry for the repo
    (`.github/actions/cargo-registry`, key = hash of `Cargo.lock` only),
    written only by the lint job on main after `cargo fetch --locked`,
    restored read-only by every other job and both deploy workflows. The
    stories bundle keeps its own registry copy (its pinned environment is
    deliberately separate; a future split is the next lever, below).
  - `SCCACHE_GHA_RW_MODE=READ_ONLY` on PR refs (sccache ≥ 0.16, pinned to
    0.18.0). Trade-off accepted: a re-push of the same PR recompiles the
    crates that PR changed and their dependents instead of hitting its own
    previous push's objects. First-push hit rates are unchanged — they were
    always main's objects — and now those objects survive.
- **2026-10-07** — over budget again, twelve days after the change above:
  10.36 GB at 06:16 UTC, and the oldest entry had been read **46 minutes**
  earlier (07:20: 10.71 GB in 8,001 entries, oldest read 35 min earlier).
  The exit criterion is ≥ 12 h. Evicted sccache objects become misses on
  top of GitHub's per-repo read rate limit (1,500 downloads/min), which
  sccache does not retry (mozilla/sccache#2821). Contents at 06:16:

  | what | entries | size | why it was that big |
  |---|---:|---:|---|
  | sccache objects | many | 5.20 GB | main's working set |
  | stories bundle | 2 | 3.45 GB | ~1.7 GB each, saved 22 min apart (two lockfile changes, #1009 and #989); 1.07 GB on 09-08 |
  | emu-esp32v3 rust-cache | 2 | 1.04 GB | ~496 MB each, two manifest generations |
  | cargo registry | 1 | 0.47 GB | the one shared entry |

  Three writers, three causes:
  - **The stories bundle grew without bound.** Keyed on Cargo.lock, each
    save started from the previous lockfile's `target/` (restore-keys
    fallback) and added to it; cargo never deletes a unit an old lockfile
    built, and none of the path packages it stored (rebuilt by every fresh
    checkout anyway) or the dev build's incremental state was ever useful
    to the next run. Restored 1,700 MB, saved 1,752 MB on the 05:28 main
    run, so ~50 MB a lockfile change, ~3 changes a day. It also still
    carried its own registry copy. Fixed: one entry per UTC day, main
    restores only that day's exact key (the day's first main run builds
    from nothing, so nothing old is carried forward),
    `scripts/ci/prune-target-cache.py` drops path packages, incremental
    state, final artifacts and dx's bundle before the save, and the registry
    comes from the shared entry. **What the whole `target/` cache buys is
    small:** PR #1022's first run, with no entry under the new key, built
    all three (fw-browser, the emulator sidecar, dx) cold in 8m28s, against
    7m42s on the 06:27 main run warm off the old 1.75 GB entry; the tools
    came by binstall in 6 s. If the pruned entry still costs more budget
    than ~45 s a stories run is worth, the next lever is caching only
    `~/.cargo/bin` (or nothing) here.
  - **`emu-esp32v3-ref` still had its own rust-cache entry.** Registry only
    (`cache-targets: false`), so the same crates as the shared entry, under
    a key hashing every Cargo.toml: a new ~496 MB generation per manifest
    change reaching main. Its sibling `emu-esp32v3` had moved to the shared
    entry on 09-25; this job was missed. Fixed: it restores the shared
    entry, and its `sccache-action` step gets the same `version:` pin as
    every other.
  - **The classic's reference image was sccache-cached at a per-commit
    path.** `build-reference-image.sh` builds it in
    `target/emu-ref/wt-<HEAD>-…` with `--remap-path-prefix=<that path>=…`
    in the target rustflags, and sccache hashes the arguments and each path
    package's `CARGO_MANIFEST_DIR`. The job builds HEAD (twice on main, with
    `--verify`), so every Xtensa compile of it was a new key: the 06:27
    main run had 302 misses and 286 writes that no later run could read
    (on the 05:28 run, 413 misses, all of them write errors), and a PR
    run's ~190 lookups could only miss. Fixed: the script calls rustc
    directly (`RUSTC_WRAPPER=""`) when the commit it builds is HEAD; a
    pinned commit keeps a stable path and its wrapper. The job's in-tree
    builds keep sccache. Measured on PR #1022: each reference build took
    1m46s without the wrapper against 2m47s with it (the 05:28 main run),
    and the job's misses fell from 302–413 to 38.

**Steady state after the 2026-09-25 change** (estimate; re-measure a week
after it lands):

| entry | size | why |
|---|---:|---|
| sccache, main's live working set | ~4.8 GB | every main-written object still present was read in the last 13 min — that is the set current jobs hit |
| cargo registry | 0.52 GB (+0.52 GB previous generation until it ages out) | one entry; a new one only when Cargo.lock changes |
| stories bundle | ~1.07 GB | measured 2026-09-08 |
| Chrome for Testing | ~0.2 GB | estimate; entry was evicted at measurement time |
| emu-ref | 0.03 GB | measured in #797 |
| **total** | **~6.6 GB, ~7.1 GB with a stale registry** | leaves ~2.9 GB of LRU headroom |

Write rate, before → after: a manifest-changing merge wrote ~3.1 GB of
registry (6 × 517 MB) → a lockfile-changing merge writes 0.52 GB; an active
PR wrote ~0.7 GB/h of sccache → 0. What remains is main's own sccache
delta, 0.1-0.2 GB/h on ordinary merge traffic (09-24 00:00-10:00 UTC) and
1.2-1.9 GB for a merge that invalidates the tree (a Cargo.lock change).
2.9 GB of headroom at 0.15 GB/h is a horizon of ~20 hours rather than 13
minutes, and a tree-wide invalidation still leaves every entry that is read
at least a few times a day.

**Exit criteria** — a week after the change, the oldest `last_accessed_at`
in the listing is at least 12 hours old, the emu-ref and stories restore
steps hit on ≥ 9 of 10 main runs, and usage sits under 9 GB. Next levers if
not: split the stories bundle's registry out onto the shared entry (~0.5 GB);
after a toolchain bump, purge the dead sccache generation by hand (a
maintainer's `gh cache delete`, never a CI step) instead of letting it
squeeze the horizon until LRU drains it.
