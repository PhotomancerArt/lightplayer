---
status: fixed
found: 2026-10-22      # profiling the emulated S3's speed (debt: emulated-s3-runs-forty-times-slower-than-real-time)
area: lp-emu-esp-common SocBus store-address record × lp-xt-emu block cache, on lp-emu-esp32s3
class: config-masked-defect
related:
  - docs/debt/emulated-s3-runs-forty-times-slower-than-real-time.md
  - lp-emu/esp/lp-emu-esp32s3/tests/block_cache_doors.rs
  - lp-emu/esp/lp-emu-esp32s3/tests/fence_contract.rs
---
# The emulated S3 runs a rewritten shader from its stale cached blocks

**Symptom.** On the emulated ESP32-S3, code rewritten in place through
SRAM1's D-bus view kept executing its **old** bytes once the hart had
cached a block for it. `tests/block_cache_doors.rs` reproduces it on the
old bus: a stub returning 111 is overwritten to return 222 and called
again; the second call returns 111. The product path is exactly this shape
— lpvm-native writes a JIT'd shader into the heap through the D-bus view
and calls it through the I-bus alias `+0x6F_0000` with no barrier — so a
shader recompiled into a heap span an earlier shader had used would have
rendered the earlier shader. No emulated walk is known to have hit it; no
silicon run can (silicon has no block cache).

**Root cause.** The block cache's invalidation contract on Xtensa is the
store address (M7 XD3): the bus records a guest store into executable
memory and the hart drops every cached block overlapping it. The bus
records the store at its **canonical** address, which on the S3 is the
D-bus one (DD81: the D-bus region carries the executable flag, the I-bus
view is an alias onto it). The hart keys its blocks by the `pc` it fetched
through — the I-bus address. The two never overlap, so the invalidation
walked the whole table and dropped nothing.

**Why nothing caught it.** `tests/fence_contract.rs` is the S3's
publish-by-store test, and it passed: its machine starts with both caches
**off**, so the D4 cache-off watch is armed, which installs a memory-cost
model, which makes `fetch_is_pure()` false — and the hart decodes no block
at all. The configuration that tests the publish is the one configuration
in which the block cache does not run.

**Fix.** The bus records each span at every alias door onto it as well as
at its canonical address (`SocBus::note_code_dirty`). The same change made
the record selective — only a store that changes a word some hart has
decoded is recorded (`CodeRecord::map`) — which is what removed the
whole-table walk from every stack push (81 % of the S3's host time). The
new test turns both caches on first, so the block cache really runs;
it fails on the old bus and passes now. The S3 machine also gained
`--no-block-cache`, so this class of bug has a free oracle
(`scripts/emu/s3-oracle.sh`).

**Lesson.** A test of a cache's invalidation must assert the cache was in
use (`block_stats()` non-empty), or a configuration that turns the cache off
passes it for the wrong reason.
