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

> **Amended 2026-08-02 — attributed, gated, and re-measured.** The 8,136 B of
> per-project growth seen after merging `origin/main` was **PR #243**
> (persistent resolution), bisected on this board; the f32 PRs and the 16-bit
> gamma fix cost nothing measurable. It is now behind
> `lpc-engine/resolver-payload-cache`, which `fw-esp32v3` leaves off. The
> table above is the pre-regression measurement and **is again current** for
> the classic image — 120 LEDs measures 18,144 B free today, 16 B better than
> the 18,128 B row, at 16 fps instead of 13.
>
> The rows below the table are the part that was wrong for a different reason
> and is corrected in place: see the per-LED and ceiling paragraphs. Details
> in `docs/defects/2026-08-01-classic-heap-regression-after-f32-merge.md`.

A loaded project costs ~79 KB (observed directly: `stop_all_projects` took the
board from 95 KB used to 16 KB used). Beyond that, **≈89.5 B per LED** — and
only ~21 B/LED of that is `DisplayPipeline`'s three `Vec<u16>` plus
`dither_overflow`. The other ~68 B/LED is engine-side; it was recorded here as
unattributed and as the single most valuable RAM lead this chip has.
**It is attributed as of 2026-08-02 — see the amendment below**, which also
corrects the "scales with `render_size`" reading.

**Practical ceiling: ~240 LEDs comfortable, ~300 at the edge, 400 impossible
— for a project whose shader is already compiled.**

That qualifier is the correction. Every row in the table above is a
*steady-state* number, read once the shader is resident, and steady state is
not the peak. Compiling GLSL on-device is a transient of tens of KB on top of
the resident project: `examples/basic` (241 LEDs, 4,092 B of GLSL) needs
44,488 B resident **plus ~65 KB of compile working set** against a 112,640 B
arena, and OOMs — while `quad-strips-v3`'s 1,267 B shader compiles in 62 ms
with room to spare. So the binding constraint on this chip is not LED count
alone but **LED count × shader size**, and no single-axis ceiling can be
quoted for a product claim. Measured 2026-08-02; see
`docs/defects/2026-08-02-classic-oom-retry-succeeds.md`.

The LED-count figure is a RAM number, not an RMT one — M4-P3 measured RMT
refill lag peaking at 20 of 64 words (31 % utilisation) with zero trips at
240 LEDs.

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
canvas pixel.

The `render_size` multiplier is real, but it lives on the **`TextureArea`
sampling path**, and it is now measured too — profiling `examples/fast`
(`texture_area`, 16×16 canvas, **one** lamp):

| bytes | owner |
|------:|-------|
| 1,024 | `ensure_texture_area_mapping` — 256 canvas pixels × 4 B `PixelMappingEntry` |
| 2,048 | `create_render_target` — 256 × 8 B RGBA16 |
| **3,072** | **per fixture, for 1 LED** |

So that path costs **12 B per canvas pixel per fixture, independent of how many
lamps are actually mapped**. `examples/fast` spends 3,072 B on one LED — 34× the
~90 B a direct-sampled LED costs. Nothing in the authoring surface warns that
widening `render_size` on a texture-area fixture is a RAM decision. Only one
project in the tree uses this path today, which is why it does not appear in the
89.5 B/LED figure.

The shape of the waste is duplication, not any single fat buffer: a lamp's
position is stored three times and its colour twice. Filed as
[`docs/debt/per-lamp-data-stored-three-times.md`](../debt/per-lamp-data-stored-three-times.md)
with a costed pay-down order.

**Taken so far (#285): 13 B/LED on the measured projects.** `DisplayPipeline`
no longer allocates `prev` when interpolation is off or `dither_overflow` when
dithering is off; `direct_points` no longer retains a 16-B-per-element
allocation for 12 B elements. Both are output-identical; the first is proven so
by a differential test against the previous allocation shape.

⚠️ **The `DisplayPipeline` part is configuration-dependent, and the
configurations differ.** Surveyed across `examples/` and `projects/`:

| output configuration | who ships it | saving |
|---|---|---|
| interpolation off, dithering off | `quad-strips-v3`, `quad60-v3`, `quad-gamma-*`, `shader-oracle` | 6 + 3 = **9 B/LED** |
| interpolation **on**, dithering off | **every other `examples/` project** | **3 B/LED** |
| both on (the `DisplayPipelineOptions` default) | nothing on disk | 0 |

`direct_points`' 4 B/LED is unconditional. So the total is **13 B/LED for the
projects the 89.5 figure was measured on** — an apples-to-apples comparison —
but only **7 B/LED for a typical example project**, which is the number that
matters for a user-facing claim. Quote the right one.

Verified in the emulator by re-running the same diff after the change: whole
image **70.2 → 66.2 B/LED**, with `direct_points` moving 16.0 → 12.0 exactly as
predicted. The `DisplayPipeline` saving cannot appear there (that type is not in
the emulator image); it is covered by a unit test asserting both buffers are
zero-length when their option is off.

> ⚠️ **The post-change figure has NOT been re-measured on silicon.** Predicted
> ≈76.5 B/LED, but that is arithmetic, not a measurement, and **nothing here
> revises the ceiling** — the LED-count × shader-size correction above still
> stands, and a smaller per-LED cost widens the LED axis of that product without
> making a single-axis ceiling quotable. Quote the measured 89.5 B/LED until a
> board measurement replaces this note.
>
> Two things blocked it, both worth knowing before the next attempt:
>
> 1. **The desk classic was in a recovery-red state from another session.** It
>    reported `last run crashed (oom) … alloc 720 bytes failed`, and the node
>    was `disabled after 3 crashes`. Clearing that ledger is a power-on-class
>    wipe which would have destroyed the other session's evidence, so the board
>    was left exactly as found.
> 2. **A `load_project` bracket does not capture the per-LED cost.** Measured on
>    the board: `quad-strips-v3` (120 LEDs) costs **53,052 B** across load
>    (101,704 → 48,652 B free). But `direct_points`, the graphics sample
>    buffers, and `DisplayPipeline` are all allocated at *tick*/output-open
>    time, not load time. The per-LED figure has to come from **steady-state**
>    free heap with the project actually rendering — which is what the original
>    18,128 B / 7,384 B two-point measurement did.
>
> **The instrumentation is ready** — #281's `[MEM] free= used= largest_free=`
> per heartbeat is the byte-precision steady-state readout, and this branch adds
> byte precision to the `load_project`/`stop_all_projects` brackets. What is
> missing is the second point:
>
> ⚠️ **`quad60-v3` no longer runs. The 240-LED row above is not currently
> reproducible.** Measured 2026-08-02 on main @ `e2272d0f8` + this branch, from
> a clean power-on boot (`level=green`): the shader node OOMs before compilation
> starts, twice, and recovery disables it.
>
> ```
> [RECOVERY] last run crashed (oom): at node:/Quad_60_v3.sh: alloc 240 bytes failed (align 1)
> [RECOVERY] oom stats: requested=240 align=1 free=548 used=112092
> [RECOVERY] last run crashed (oom): at node:/Quad_60_v3.sh: alloc 1440 bytes failed (align 8)
> [RECOVERY] oom stats: requested=1440 align=8 free=2024 used=110616
> ```
>
> `used=112092` of a 112,640 B arena is genuine exhaustion, not fragmentation,
> and it happens *with* this branch's 13 B/LED reduction applied. So the
> two-point method has no 240-LED point on this arena, and the per-LED figure
> stays unmeasured.
>
> This is the same drift as the loaded-`used` table above, one step further —
> far enough to cross the cliff.
>
> ✅ **Answered same day: `quad60-v3` runs on the +65,536 B reclaim**
> (`claude/classic-jit-region-rightsize`, PR #288) — `level=green`, 201 s soak,
> zero OOM lines, `free=67,588`. The 240-LED row is recoverable, on the arena
> that will actually ship rather than on the one that OOMs. `examples/basic`
> also compiles *and runs* there (183 ms, `[MEM] used=67,412`,
> `[JIT] used=6,516`), where this ADR previously recorded it OOMing.
>
> ⚠️ But the two-point measurement is **still** blocked, for a different reason:
> `lp-cli upload` cannot establish a clean single-project state (see the defect
> note below), so `used` after switching projects reads as two projects resident
> rather than a 120-LED steady state. The per-LED figure therefore stays
> derived. The blocker is now precisely "reach exactly one loaded project
> without a reflash".
>
> ⚠️ Also observed: **`lp-cli upload` cannot switch a board out of a
> crash-disabled project.** The deploy acks, `[MEM]` never moves, and the board
> keeps serving the old project (three attempts, `--wait-timeout` to 90 s). A
> reflash does not help on its own because the startup project persists and it
> boots straight back into the OOM loop.
>
> ⚠️ **What actually stopped the third attempt: `espflash` wedged.** After two
> flashes of this image succeeded earlier the same session, the next two both
> hung at chunk `1/1019` and had to be killed. The chip stayed healthy at ROM
> level (`espflash board-info` answers normally, correct MAC), but **the app
> partition may be partially erased, so the board should be replugged and
> reflashed before it is trusted.** Same family as the S3 wedge in memory
> `esp32s3-espflash-serial-wedge` — a hung `espflash` is a replug, not a retry.
> Retrying without a replug reproduced the hang exactly.

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
