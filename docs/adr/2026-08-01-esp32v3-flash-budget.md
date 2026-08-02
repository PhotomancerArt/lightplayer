# Classic ESP32 keeps the C6's 4 MB table, and the binding constraint is RAM

- Status: accepted
- Date: 2026-08-01
- Context: M6 of `2026-07-31-1444-classic-esp32-bringup` (classic ESP32 / LX6
  bring-up). Sibling of `2026-07-28-esp32c6-flash-budget.md` and
  `2026-07-30-esp32s3-partition-floor.md`.

## Context

`fw-esp32v3`'s `partitions.csv` copied the C6's shape verbatim — 3 MB
`factory` + 960 KB `lpfs`, exactly 4 MB — as roadmap decision Q7, on the
reasoning that the classic's flash budget is constrained like the C6's rather
than like the S3's. That was a prediction made before the app layer existed.
The S3 made the opposite call three days earlier and moved to an 8 MB floor,
so the question is live: does the classic need to follow?

All numbers below are measured on this branch, not estimated.

## Flash: the prediction was wrong, and in the comfortable direction

| build | image | `.text` | `.rodata` |
|---|---|---|---|
| hello (`--no-default-features --features esp32`) | 84,016 B | 18,409 | 6,712 |
| radio probe (`+radio_ram_probe`) | 449,872 B | 319,317 | 29,636 |
| **server + JIT + ws281x (shipping)** | **1,707,792 B** | 1,441,693 | 233,240 |

Against the 3 MB (3,145,728 B) `factory` partition that is **1,437,936 B of
headroom — 46 % free**, comfortably past the 65,536 B margin
`just fw-esp32v3-size-check` enforces.

Cross-chip, measured the same way (`espflash save-image` at each chip's real
flash size; S3 and C6 from local ELFs built 2026-07-31, so they are a snapshot
of that day rather than of this branch):

| chip | image | app partition | headroom |
|---|---|---|---|
| esp32c6 (rv32imac) | 2,861,200 B | 3 MB | ~284 KB |
| **esp32v3 (LX6)** | **1,707,792 B** | **3 MB** | **1,437,936 B** |
| esp32s3 (LX7) | 1,699,072 B | 6 MB | ~4.6 MB |

The classic's image lands within **8,720 B of the S3's** — the two Xtensa
builds are effectively the same size — while the C6 is 1.15 MB larger. Two
factors plausibly account for that gap and this ADR does **not** claim a split
between them, because nothing here measured one: the C6 is the *unwinding*
tier (it links `unwinding` and carries `.eh_frame`) where both Xtensa builds
are abort tier, and rv32imac and Xtensa have different code densities.

The practical consequence is the only part that needs deciding, and it is
unambiguous either way.

## Decision

**The classic ESP32 keeps the 4 MB table unchanged.**

```
nvs,      data,  nvs,     0x9000,   0x6000,
phy_init, data,  phy,     0xf000,   0x1000,
factory,  app,   factory, 0x10000,  0x300000,   # 3 MB
lpfs,     data,  spiffs,  0x310000, 0xF0000,    # 960 KB
```

Ends at 0x400000 of 0x400000 — no slack, by construction, and none needed. Q7
stands. Unlike the S3, this chip does **not** narrow its supported hardware:
any 4 MB N4-class module runs the shipping image with 46 % of the app
partition spare.

**The serde-surface lever is NOT pulled for this chip** (roadmap Q3 — go/no-go
below).

## Flash: where the image actually goes

`cargo bloat --profile release-esp32v3 --crates`, top of the `.text` section
(1.4 MiB of a 2.2 MiB file):

| crate | share of `.text` | size |
|---|---|---|
| `lps_glsl` | 13.1 % | 184.5 KiB |
| `lpc_model` | 12.3 % | 174.1 KiB |
| `lpc_engine` | 12.0 % | 169.3 KiB |
| `lpa_server` | 7.8 % | 109.5 KiB |
| `lpvm_native` | 6.4 % | 90.1 KiB |
| `lps_builtins` | 5.2 % | 73.2 KiB |
| `core` | 5.0 % | 71.1 KiB |
| `lpc_wire` | 3.9 % | 55.0 KiB |
| `serde_core` | 3.8 % | 53.1 KiB |
| `lpc_registry` | 3.7 % | 52.0 KiB |

The on-device shader toolchain (`lps_glsl` + `lpvm_native` + `lps_builtins` =
347.8 KiB, 24.7 %) is the largest single thing this firmware carries, and it is
the feature that distinguishes the product. It is not a candidate for removal.

## Q3 — serde-surface go/no-go: **NO-GO for the classic**

The serde surface (`lpc_model` + `lpc_wire` + `serde_core` + `serde_json` +
`ser_write_json`) totals **331.9 KiB** of `.text`. Memory
`serde-surface-is-the-flash-lever` records why that matters on the C6: at
~284 KB of headroom, that lever is the difference between fitting and not.

On the classic it buys nothing anyone needs. Pulling all of it would take
headroom from 1,437,936 B to ~1,777,000 B — from 46 % free to 56 % free, on a
chip that is not flash-constrained. **This is C6-only pressure.** If the lever
is ever pulled it should be justified by the C6's budget and merely inherited
here, never the reverse.

Recommendation only; no size-reduction work is in scope for this roadmap.

## RAM is the real constraint, and it binds hard

Flash is not this chip's problem. DRAM is.

`dram_seg` is **192 KB total** (`0x3FFB_0000..0x3FFE_0000`) and `.data`,
`.bss` and `.stack` are strictly zero-sum within it — `.stack` takes whatever
the other two leave (esp-hal's `stack.x`).

Shipping image, measured:

| section | size |
|---|---|
| `.data` | 19,420 B |
| `.bss` (incl. the 112,640 B heap arena) | 134,296 B |
| `.stack` | 42,888 B |
| **sum** | **196,604 B of 196,608** |

⚠️ **"As high as it links" is not the ceiling.** A 160 KB arena fails the link
by 4,064 B, so the hard limit is ≈155.9 KB — at which `.stack` is *zero* and
the board cannot run. The real constraint is stack headroom for the Xtensa
windowed ABI's large frames and the recursive GLSL parser.

The `lp-recovery` RTC ledger costs **nothing** from this budget: its 976 B live
in RTC fast RAM (`0x3FF8_0000`, 8 KiB, a separate segment). Only its code came
out of `.stack`, 504 B.

### Runtime heap, measured on silicon (112,640 B arena)

| state | free | used |
|---|---|---|
| idle, no project | 102,156 B | 10,484 B |
| `quad-strips-v3` (4 × 30 = 120 LEDs) | 18,128 B | 94,508 B |
| `quad60-v3` (4 × 60 = 240 LEDs) | **7,384 B** | 105,256 B |
| `quad-equal100-v3` (4 × 100 = 400 LEDs) | — | **OOM** |

> ⚠️ **These numbers are from commit `08779e059` and have already moved.**
> Re-measured after merging `origin/main` (2026-08-01, same board, same
> projects): per-project heap grew **8,136 B** — 120 LEDs now leaves 9,992 B
> free, and **240 LEDs OOMs where it previously ran**. Idle heap is unchanged,
> so it is per-project allocation, not static growth; `tick` simultaneously
> dropped 69 ms → 47 ms. The 16-bit gamma fix (#252) is ruled out by direct
> measurement (4 bytes). See
> `docs/defects/2026-08-01-classic-heap-regression-after-f32-merge.md`.
>
> The **method and the shape of the conclusion stand** — heap binds, flash does
> not, and LED counts must be quoted from RAM. The **absolute ceiling below is
> optimistic by roughly 91 LEDs** until that regression is attributed and
> either accepted or reversed.

A loaded project costs ~79 KB (observed directly: `stop_all_projects` took the
board from 95 KB used to 16 KB used). Beyond that, **≈89.5 B per LED** — and
only ~21 B/LED of that is `DisplayPipeline`'s three `Vec<u16>` plus
`dither_overflow`. The other ~68 B/LED is engine-side; it was recorded here as
unattributed and as the single most valuable RAM lead this chip has.
**It is attributed as of 2026-08-02 — see the amendment below**, which also
corrects the "scales with `render_size`" reading.

**Practical ceiling: ~240 LEDs comfortable, ~300 at the edge, 400 impossible.**
For a WLED-class product claim that number matters more than the channel count,
and it is a RAM number, not an RMT one — M4-P3 measured RMT refill lag peaking
at 20 of 64 words (31 % utilisation) with zero trips at 240 LEDs.

### Amendment 2026-08-02 — the per-LED cost is attributed

The ~68 B/LED above is no longer a mystery. Attributed with
`lp-cli profile --collect alloc --mode all`, diffing the live-allocation set of
`quad-strips-v3` (120 LEDs) against `quad60-v3` (240 LEDs) by demangled
callsite — a host measurement, no board time:

| B/LED | Owner |
|------:|-------|
| 25.6 | resolved `MappingConfig::PathPoints` — `MapSlot<u32, XySlot>`, 24 B per lamp for 8 B of coordinate |
| 16.0 | `direct_points` (a second copy of the same positions) |
| 8.0 | graphics `sample_points` (a third copy, in pixel space) |
| 8.0 | graphics `sample_out` (RGBA16 results) |
| 6.0 | `OutputNode::control_samples` |
| 6.0 | runtime buffer bytes (a second copy of the same colours) |
| **69.6** | **engine-side total, measured in the emulator** |
| 21.0 | `DisplayPipeline` — absent from the emulator image, known from source |

Reconciliation: 67.5 engine-side (excluding 2.7 B/LED of map2d JSON text held
in the *emulator's* RAM filesystem, which is flash-resident on silicon) + 21.0
`DisplayPipeline` = **88.5 B/LED predicted against 89.5 measured**, within ~1 %.

⚠️ **"Scales with `render_size`" was wrong** for the projects these numbers come
from. Both use `sampling: "direct"`, which allocates per *mapped lamp*, not per
canvas pixel. The `render_size`-sized allocations — `precomputed:
Vec<PixelMappingEntry>` at 4 B per canvas pixel plus a W×H×8 B RGBA16 render
target per fixture — are on the `TextureArea` path only. That path is a real
multiplier (a 60×4 canvas driving 60 LEDs allocates 240 entries and a 7,680 B
texture per fixture) and remains **unmeasured**.

The shape of the waste is duplication, not any single fat buffer: a lamp's
position is stored three times and its colour twice. Filed as
[`docs/debt/per-lamp-data-stored-three-times.md`](../debt/per-lamp-data-stored-three-times.md)
with a costed pay-down order.

**Taken so far (#282): 13 B/LED.** `DisplayPipeline` no longer allocates `prev`
when interpolation is off or `dither_overflow` when dithering is off (9 B/LED
for outputs configured that way — which is every output in the test projects),
and `direct_points` no longer retains a 16-B-per-element allocation for 12 B
elements (4 B/LED, unconditional). Both are output-identical; the first is
proven so by a differential test against the previous allocation shape.

> ⚠️ **The post-change figure has NOT been re-measured on silicon.** The desk
> classic was held by another session throughout this work. Predicted ≈76.5
> B/LED and a ceiling near 235 LEDs, but **treat the ceiling line above as the
> current quotable number until a board measurement replaces this note** — the
> whole point of this ADR is that LED counts are quoted from measured RAM.

### Radio, if it is ever attempted

M2-P3 measured `wifi::new` + STA at **44,244 B of heap** plus ~28 KB static
and ~390 KB flash. Against an app that already uses ~94 KB of a 110 KB arena,
WiFi + JIT does not fit without a RAM diet of serde-surface scale. That is the
measured price of D1's "radio-off for v1", recorded here so the future attempt
starts from a number rather than a hope.

⚠️ Note the asymmetry this ADR exists to make explicit: **the RAM diet WiFi
would need is a different lever from the flash diet the C6 needs**, even though
both point at `lpc_model`/serde. Flash headroom on this chip is 1.4 MB; heap
headroom at four channels is 7 KB.

## Consequences

- Any 4 MB N4-class classic module is supported. No hardware narrowing.
- The flash size check keeps its 65,536 B margin; at 1.4 MB of headroom it is
  a tripwire for accidents, not a real constraint.
- LED-count claims for this chip must be quoted from the RAM ledger, not from
  the RMT channel count.
- The ~68 B/LED engine-side cost is the next RAM lead. It is shared with the
  S3 and C6, which have the arena to absorb it — so it is latent there, not
  absent, exactly like the per-channel LUT this roadmap removed.

## Reproducing

```bash
just fw-esp32v3-size-check
cd lp-fw/fw-esp32v3 && cargo bloat --profile release-esp32v3 --crates -n 18
```

Per-variant images: `touch src/main.rs` before each build (feature-driven cfg
is not reliably tracked on this crate), then `espflash save-image --chip esp32
--flash-size 4mb <elf> <out>`. Section sizes via `xtensa-esp32-elf-size -A`.
Runtime heap comes from the device's own heartbeat `memory` field.
