---
status: carried
since: 2026-06-12      # ChunkedVec lands with a 64-ELEMENT bound (b9f03cd9b)
logged: 2026-08-02
area: cross-cutting — lp-collection, lps-glsl, lpvm-native regalloc
related:
  - ../defects/2026-08-02-classic-oom-retry-succeeds.md
  - per-frame-optimisations-are-unpriced-in-ram.md
  - output-channel-led-cap-silent-truncation.md
---
# Bounds asserted in the unit that was convenient to count

**Shape** — a limit is written and tested in the unit that was easy to
reach for, not the unit its **consumer** actually cares about. The
arithmetic is always correct; the unit is wrong, so the limit does not
constrain the thing it names. Every instance so far also **passed its
tests**, because the test data happened to make the wrong unit and the
right one coincide.

The defence is two rules, and the second is the one that generalises:

> A bound must be asserted in the unit its **consumer** cares about —
> bytes for the allocator, slots for the frame layout — not the unit
> that was convenient to count. And it must be tested against data
> chosen to be *inconvenient*: a type whose size is not a power of two,
> a count that is not a divisor of the bound.

This is structural rather than a run of unrelated bugs because the two
subsystems involved share nothing — a compiler frontend collection and
a register allocator — and neither author could see the class from
their own instance. It is filed at n=3 deliberately; see the log.

**Carrying cost** — the failure mode is not "the bound is exceeded by a
bit", it is that the bound is silently absent. Costs so far: a
6,144-byte allocation from the collection whose entire purpose was to
avoid large contiguous allocations, on the one chip that notices; a
day of the classic ESP32's OOM being read as an allocator bug; and a
`u8` slot counter that miscompiles rather than crashes. The tests give
no warning, so each instance is found by measurement or by a board.

**Severity ceiling: silent wrong answers, not crashes.** Two of the
three instances announced themselves (an assert, an OOM). The spill-slot
one does not: with overflow checks off it wraps to slot 0 and two live
values share a slot, which is miscompiled shader code on a chip that
compiles on-device. Assume the silent variant when triaging this class.

**Workarounds** — until the rule is enforced somewhere:

- Assert against the *consumer's* observable. For an allocation that
  means `Vec::capacity() * size_of::<T>()`, never `len()` and never the
  element count.
- Pick test element types that are hostile: sizes that are not powers
  of two and do not divide the bound (96, 40, 20, 12 — not just `i32`).
  `real_chunk_allocations_stay_within_the_byte_bound` in
  `lp-base/lp-collection/src/chunked_vec.rs` is the shape to copy.
- Get real device type sizes without hardware:
  `cargo rustc -p <crate> --target riscv32imac-unknown-none-elf -- -Zprint-type-sizes`.
  Both `ChunkedVec` instances were invisible until `HirExpr` was
  measured at 96 B rather than assumed small.
- Where a counter's width *is* the bound, say so at the type and check
  the increment; `+= 1` on a `u8` is not a bound, it is a wrap.

**Incident log**

- **2026-08-02 — `ChunkedVec` bound in elements** (`lp-collection`,
  introduced 2026-06-12 in `b9f03cd9b`). `CHUNK_SIZE = 64` *elements*,
  calibrated against 20-byte `LpirOp` (1,280 B/chunk, inside the "~2–4 KB" its comment
  claimed). `lps-glsl` later reused it for `HirExpr`, **96 B** on a
  32-bit target, so a chunk silently became **6,144 B** — and the
  doubling path allocates 384 → 768 → 1,536 → **3,072** → 6,144 B. The
  3,072 B step is the allocation that OOM'd the classic ESP32
  (`../defects/2026-08-02-classic-oom-retry-succeeds.md`). Consumer unit
  is bytes the allocator hands out; the bound's unit was elements.
  Fixed in #284.

- **2026-08-02 — the *fix* repeated the class one layer down**
  (`lp-collection`). `CHUNK_BYTES / size_of::<T>()` = 10 for a 96-byte
  element looks byte-bounded, but the last chunk is a `Vec` grown by
  pushing and `RawVec` doubles from `MIN_NON_ZERO_CAP` — capacity cannot
  stop at 10, it reaches 16, i.e. **1,536 B against a 1,024 B bound**.
  Every existing test still passed: they all use `i32`, whose 256 is a
  power of two, so `len` and `capacity` coincided. Fixed by rounding
  `CHUNK_SIZE` down to a power of two, plus the hostile-element-size
  test named above. The instructive part is that this one was committed
  *by someone who had just diagnosed the same class* — knowing the rule
  is not the same as having a test that enforces it.

- **2026-08-02 — `SpillSlots::next_slot` is a `u8`**
  (`lp-shader/lpvm-native/src/regalloc/spill.rs:63`). `get_or_assign`
  does `self.next_slot += 1` unguarded, so a function needing >255 spill
  slots wraps. Consumer unit is "slots the frame layout can address";
  the counter's unit is "whatever fits in a byte". **Panics where
  overflow checks are on, silently wraps to slot 0 where they are not**
  — two live values then share one spill slot: miscompiled shader code,
  not a crash. Reachable from user GLSL, shared by both ISAs. Found by
  a synthetic 220-statement `render()`; running in its own session.

  Filed as a task chip at the time, correctly: at n=1 the register's
  own bar ("todos, feature ideas, and one-off deferrals do not belong
  here") excludes it. At n=3, across two unrelated subsystems, inside
  one day, "structural and recurring" is satisfied on the register's own
  terms — which is the argument for the entry existing. **The class was
  invisible from any single instance.**

**Exit criteria** — one of:

1. A lint or test helper that makes the consumer-unit assertion the
   default rather than a thing to remember — e.g. a shared
   `assert_allocation_within::<T>(bound)` used by every bounded
   collection, and counter types that carry their own limit rather than
   relying on a primitive's width.
2. Or, if that proves not worth building: every currently-known bounded
   quantity audited once against its consumer's unit and given a
   hostile-data test, with this entry retired and the rule moved into
   `CONTRIBUTING.md`.

Retire when a new instance of this class would be caught by CI rather
than by a board or a measurement harness.
