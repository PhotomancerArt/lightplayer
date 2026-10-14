# lp-nor-sim

A deterministic, sans-IO model of SPI NOR flash that loses power on demand.
It is the ground the storage testbed (`tools/lp-store-bench`) runs every
candidate store on, so its own correctness matters more than its speed.
`no_std` + `alloc`, no dependencies beyond `embedded-storage`'s traits.
Nothing here is linked into firmware.

MIT, as a unit with the rest of `lp-emu/` (see `../LICENSE-MIT`): the host
testbed and the emulated C6's flash share this one model, and nothing in this
crate may depend on an AGPL workspace crate (`just lint-emu-fence`).

## What it models

- **Geometry** (`NorGeometry`): sector count and size, program page size.
  Default: the C6 data partition, 128 × 4 KiB, 256 B pages.
- **Erase** sets every bit of one sector to 1. **Program** clears bits
  (`stored = stored & data`); a program longer than a page, or crossing a
  page boundary, runs page by page.
- **0→1 violations:** a program asking a cleared bit to become 1 (real NOR
  cannot; the result is not what the store wrote) is counted in
  `NorStats::violations_0_to_1`, and panics in debug builds when the sector
  is *pristine* (no tear since its last full erase) — there it can only be a
  store bug. Writing 1s over 1s and 0s over 0s is fine.
- **Operations:** every program page and every sector erase is one op; reads
  are not. A `FaultPlan { cut_after, tear, seed }` cuts power when the
  op count since the plan was installed reaches `cut_after`: that op is torn,
  and every later op — reads too — returns `NorError::PowerLost` until
  `power_cycle(next_plan)`, which keeps the cells exactly as they are.
- **Tear models** (`TearModel`), for a torn program:
  - `Clean` — the op does nothing;
  - `BytePrefix` — the first *n* bytes land, byte *n* gets a random subset
    of its intended 1→0 clears, the rest nothing;
  - `RandomBits` — a random subset of the page's intended clears lands.
- **Torn erases** (`BytePrefix` and `RandomBits`), in one of three seeded
  shapes: a byte-wise mix of old bytes, `0xFF` and weak bits; a sector that
  **reads all `0xFF` but carries weak bits**; erased up to a point, old
  after it, weak around the edge. A **weak** bit reads as a fresh random
  value on every read until the sector is erased in full. This is the
  realistic nasty case: a sector that "reads erased" may not be, so a store
  must only trust a sector it finished erasing *and then marked*.
- **`Calibrated`** (`calibrated_tear.rs`) — tears shaped and weighted the
  way a real part tore: 500 power cuts on CX1, a generic C6 board with flash
  JEDEC `0x464016` (`docs/reports/2026-10-08-c6-nor-tear-calibration.md`;
  fitted on the first 200, confirmed at 500 on every share it had, re-fitted
  on the 500 when they showed two things it could not draw). A torn
  **program** lands a prefix of exactly what was asked, ending on the mask
  ROM's 32-byte command (58 of 79) or on a 4-byte word inside one (19 of
  79) — or, rarely, **inside a word** (2 of 79): whole words, then one word
  with only some of its clears landed and a weak bit on one that did not,
  then nothing. A cut at an **erase** is one of five states the part passes
  through (it pre-programs the sector to `0x00`, then lifts it): a
  word-aligned `0x00` run from the front with the old data after it (17 of
  418), all `0x00` (63), a residue of zeros spread over the sector with weak
  bits (69, drawn from the observed table), reads `0xFF` with a few weak
  bits (4), and reads `0xFF` with **no** weak bit (264) — or (1 of 418) the
  cut landed **before the erase started**: the sector exactly as it was,
  not even tainted (`cut_before_erase` is where the cut landed, not an erase
  shape; `NorStats::erase_cuts_before_start` counts it, `torn_erases` does
  not). The weights are `TearMix::CX1`; `NorFlashSim::set_tear_mix` replaces
  them (e.g. `TearMix::CX1.erase_only(EraseShape::AllZero)` makes every torn
  erase read `0x00`). `CalibratedZeroing` … `CalibratedReadsFf` — named
  `calibrated_zeroing`, `calibrated_all_zero`, `calibrated_erasing`,
  `calibrated_reads_ff_weak`, `calibrated_reads_ff` — forces every torn
  erase to one shape, so a sweep meets that state at every erase cut instead
  of at about one in five; `calibrated_before_erase` lands every erase cut
  before its erase, and `calibrated_mid_word` tears every program inside a
  word (the other family keeps the calibrated mix). None of these is in
  `TearModel::ALL` (the drivers' default list); name one
  (`TearModel::from_name("calibrated")`, `TearModel::NAMED`) to run it. The
  three guessed models are unchanged.
- **Counters** (`NorStats`): ops, program calls/pages/bytes, erases per
  sector, read calls/bytes, violations, torn ops, torn erases, erase cuts
  before the erase started.
- **Cheap clones:** cells live in one `Arc` buffer per sector, so a sweep
  runs a workload prefix once and forks it per cut point.
- **Read watchdog** (`set_read_budget`): reads fail with `Watchdog` past a
  budget, so a store looping on bad flash ends instead of hanging a sweep.
- **`embedded-storage`** `ReadNorFlash`/`NorFlash`/`MultiwriteNorFlash`
  (READ_SIZE 1, WRITE_SIZE 1, ERASE_SIZE 4096), plus the plain `read` /
  `program` / `erase_sector` API.

Everything random comes from the plan's seed (SplitMix64): same seed and
same calls, same bytes.

## What it does not model

Timing of any kind, read disturb, retention / bit rot, wear-out (erases are
counted, never fail), multi-plane or suspend/resume, the cache and XIP
mapping, and the SPI bus. A cut between two operations and a cut that tears
one are the only failures.
