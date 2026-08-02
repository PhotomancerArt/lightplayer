---
status: fixed
found: 2026-08-01      # how: hardware-walk
fixed: this change
area: lpc-engine (dataflow/resolver) + fw-esp32v3
class: capacity-regression
related:
  - 2026-08-01-classic-rmt-open-fault.md
  - ../adr/2026-08-01-esp32v3-flash-budget.md
  - ../debt/per-frame-optimisations-are-unpriced-in-ram.md
---
# Classic ESP32: per-project heap grew 8,136 B, cutting the LED ceiling by ~90

**Board:** DOM-Z-102 (classic ESP32 rev v3.1), `fw-esp32v3`, 110 KB arena.

## Symptom

`projects/test/quad60-v3` (4 × 60 = 240 LEDs) ran with 7,384 B of heap to
spare at G-M4. After merging `origin/main` into `claude/infallible-bose-84a52e`
it OOM'd — `cause=oom "alloc 360 bytes failed"` → safe mode — and later failed
at project *load*.

Identical project (`quad-strips-v3`, 4 × 30 = 120 LEDs), clean boot with
auto-load, same board:

| | pre-merge | merged main | delta |
|---|---|---|---|
| free heap | 18,128 B | **9,992 B** | **−8,136 B** |
| fps | 13 | **20** | +54 % |
| `tick` | 69 ms | 47 ms | −22 ms |

**Idle heap was unchanged** (102,156 B → 102,144 B with no project), so this
was never static or `.bss` growth. Faster *and* fatter is the signature of
work being cached rather than recomputed, and that is exactly what it was.

## Root cause

**PR #243, "resolver persists resolution across frames"** — and nothing else.
Bisected on silicon 2026-08-02 by merging successive `main` commits into the
branch tip that measured 18,128 B (`cee3ab922`) and reading the heartbeat:

| point | free heap |
|---|---|
| `cee3ab922` (pre-merge tip) | 18,128 B ← reproduces the row above exactly |
| + main through #241 (`a03ddd7c6`) | 17,928 B |
| + **#243** (`9eff1d8cd`) | **9,992 B** ← reproduces the other row exactly |
| + everything through #252 (`c6ca6ef9e`) | 9,992 B |
| `origin/main` @ `f6b783ec2` | 9,852 B |

Two points bracketed it; the other twelve merges in the window — including
all four f32 PRs (#241/#249/#251/#253), the 16-bit gamma (#252) and the
io_task JSON lift (#245) — cost nothing measurable. The original candidate
list was wrong: "changes shader codegen" was a plausible story, not evidence.

**The allocation**, measured on the device rather than inferred, for
`quad-strips-v3` (56 interned queries):

| table | live entries | payload bytes |
|---|---|---|
| `structural` (authored-def reads, deep copies) | 27 | 4,606 |
| `values` (this frame's productions) | 18 | 2,664 |
| the three index `Vec`s | — | 2,648 |

≈ 9.9 KB, before `Rc` headers and allocator rounding, where the pre-#243
resolver held **none** of it between frames. The rest of the difference is
that a resolver which drops each answer as it is consumed never holds all 56
answers at once; this one does, by design.

The regression is not a leak and not a mistake — it is a trade that was
priced on a part with memory to spare and then shipped to one without.

## Not fragmentation — measured, not assumed

The natural reading of "requested 3,072, free 5,304, failed" is a shredded
heap. It is wrong here. `fw-esp32v3`'s OOM report now carries
`largest_free`, binary-searched out of the allocator (`largest_free_block`),
and on this board it tracks `free` to within ~13 bytes at every sample —
idle, loaded, and at the moment of failure:

```
allocation failed: requested=3072 align=4 free=2672 used=109964 largest_free=2662
```

The classic's heap is essentially one block. Its OOMs are exhaustion.

## Fix

`resolver-payload-cache`, a removal-only Cargo gate on `lpc-engine`
forwarded by `lpa-server`, defaulting **on**. It splits the cache into the
two different bargains it had been carrying under one name:

- **decisions** (routes, the query intern table, `static_paths`) — no
  resident cost, worth 11 ms of the 24 ms;
- **payloads** (the two value tables) — worth the remaining 13 ms, for
  8,368 B.

`fw-esp32v3` omits the gate. `fw-esp32s3` and `fw-esp32c6` list it, so
nothing changes for them or for any host build.

Measured on the DOM-Z-102, `quad-strips-v3`:

| | free heap | fps | `tick` |
|---|---|---|---|
| before the cache existed (`cee3ab922`) | 18,128 B | 13 | 69 ms |
| **this change** | **18,144 B** | **16** | **58 ms** |
| the same firmware, gate on | 9,776 B | 21 | 45 ms |

(The last two rows are the same build with one Cargo feature flipped, on
this branch merged with `main` at `803157992` — so they also carry the
WS281x runtime block planner, which is why they are a few ms slower and
~76 B tighter than the `f6b783ec2` A/B the bisect above used.)

The board ends up ahead of where it was before the regression on *both*
axes, and the shader still compiles on-device in 62 ms with all four RMT
channels open.

## Regression coverage

`cached_and_uncached_resolution_agree_frame_for_frame`
(`lpc-engine/src/engine/resolution_persistence_tests.rs`) now runs the same
scene in three modes — cached, uncached, and decisions-only — and demands
they agree frame for frame. A shipped mode that is not in the differential
is an untested mode, and decisions-only is the one most able to go wrong:
it is the only mode where a hit and a miss can disagree *within* one frame.

There is no automated guard on the heap number itself. The measurement
needs silicon; see Reproduce below.

## Lesson

A cache is priced in two currencies and this codebase had only been reading
one of them. #243 was measured, defensible, and a clear win on its own
terms — nobody wrote down what it cost in bytes because on the S3 and the C6
nothing noticed. The classic noticed within a day, exactly as it did for the
per-channel white-point LUT. **The classic is the family's canary for
per-project heap; a per-frame optimisation that lands without a heap number
next to its cycle number is unpriced.**

The second lesson is about the report. `free` on a first-fit heap is a sum,
and a sum cannot answer "will this fit". Adding `largest_free` cost nine
lines and permanently separates two failure modes with different fixes — and
it immediately paid for itself twice: it ruled out fragmentation here, and
it surfaced the `retry_ok` anomaly below, which nobody would have looked for.

## Still open: `examples/basic` does not compile on this board

Reclaiming 8,368 B does **not** make `examples/basic` (241 LEDs, 4,092 B of
GLSL) compile on the classic, because at first boot the compile happens
*before* the resolver has cached anything — the reclaimed bytes are not
available yet. It fails the same way before and after this change, and it
also failed at M3 (`7aff9b10e`), so this is not a regression: that shader
has never compiled on this chip. The M3 proof used `quad-strips-v3`'s
1,267 B shader, which still works.

The failure is worth its own record because of what the new instrument says
about it:

```
allocation failed: requested=3072 align=4 free=3508 used=109128 \
  largest_free=3495 retry_ok=true context=shader node: compile
[OOM] RETRY SUCCEEDED: the same 3072-byte request fits now.
```

`retry_ok=true` means the allocator refused a request it satisfies
microseconds later, with no intervening free. The frame below it is
`ChunkedVec<lps_glsl::hir::types::HirExpr>::push` → `RawVec::grow_one`
inside `TypeCtx::type_call` — the GLSL type checker doubling a chunk while
type-checking the `psrdnoise` call chain. Whatever the mechanism (a
first-fit edge at the very bottom of the arena is the leading guess), the
board is out of memory in every practical sense: 44,488 B of project
resident plus a ~65 KB compile working set against a 112,640 B arena.
Filed separately as
`2026-08-02-classic-oom-retry-succeeds.md`.

## Reproduce

```bash
just build-fw-esp32v3
cd lp-fw/fw-esp32v3 && espflash flash --chip esp32 --port <port> \
  --partition-table partitions.csv --flash-size 4mb --baud 921600 \
  --after hard-reset ../../target/xtensa-esp32-none-elf/release-esp32v3/fw-esp32v3
espflash erase-region --port <port> 0x310000 0xF0000
espflash reset --port <port>          # power-on class: also voids any path quarantine
cargo run -p lp-cli -- upload projects/test/quad-strips-v3 serial:<port>
```

Then read `[MEM] free=… used=… largest_free=…`, which `fw-esp32v3` prints
once per heartbeat. The fd must be held open across `stty` — a bare `stty`
then `cat` reopens the port and loses the baud:

```bash
exec 3<> /dev/cu.wchusbserial1130
stty -f /dev/cu.wchusbserial1130 921600 raw -echo clocal
timeout 30 cat <&3 > out.log
exec 3<&-
```

⚠️ Every `espflash` touch costs the ledger two sub-second boots; after a few
the board latches safe mode and skips auto-load. `espflash reset` is a
power-on-class ledger wipe, which is the clean way to start a measurement.
