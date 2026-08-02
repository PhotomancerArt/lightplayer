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

### Amended 2026-08-02 — the heap is now 178,176 B, in two regions

Everything above concerns `dram_seg`, and every byte in it is zero-sum with
`.stack`. It is no longer the whole heap.

esp-hal also declares **`dram2_seg`** (`0x3FFE_7E30`, 98,768 B), which no
linker section targets — esp-idf uses the same span as heap. This image could
not, because `lpvm_native::codemem_esp32::CodeRegion::ESP32_DEFAULT` sat in the
middle of it, reserving **92 KiB of SRAM1 for JIT'd shader code**. That size
was never measured; it was chosen as a comfortable span.

It has now been measured. `lpvm-native`'s `tests/xt_classic_codemem_corpus.rs`
compiles every shader in `examples/` and `projects/` through the device's own
pipeline at the device's own settings (Q32, fuel on):

| figure | measured |
|---|---|
| largest single shader (`examples/basic`, 4,092 B GLSL) | 6,516 B |
| mean over 27 shaders | 3,348 B |
| worst real project (`fyeah-button`, 2 shader nodes) | 10,260 B |
| + one keep-last-good recompile copy | **16,776 B** |

The last row is the peak, because `shader_node.rs` keeps the old program
resident while its replacement compiles. These are device figures, not a host
estimate: the classic reported 2,444 B for `examples/shader-oracle` and M3
measured 2,032 B for `quad-strips-v3`, and the test reproduces both exactly.

Builtins are **not** resident in the region — `jit_builtin_code_ptr` hands the
linker addresses of functions already in the firmware's `.text`, so a shader
that calls `sin` costs a 4-byte literal slot, not a copy of `sin`. That is why
per-shader cost stays in the low kilobytes regardless of how much of GLSL a
shader touches.

**The region is now 32 KiB** — 1.95× the measured peak, 5.03× the largest
single real shader, ~9.8 shaders at the corpus mean. The remaining **64 KiB is
a second `esp_alloc` region** (`main.rs`'s `add_sram1_heap_region`). The
boundary is computed from `CodeRegion::reclaimable_heap_span()` and
const-asserted to abut the region exactly, replacing the prose warnings that
previously lived in two files and could not fail to compile. `dram2_seg`
carries no ROM hazard: all four esp-hal ROM reservations sit *below* its
origin, the last ending exactly at it.

Measured on the desk DOM-Z-102, boot green, `safeMode=false`:

| state | before (112,640 B) | after (178,176 B) |
|---|---|---|
| total heap | 112,640 B | **178,176 B** (+65,536) |
| idle, no project | 102,156 B free | **167,212 B free** (+65,056) |
| `quad-strips-v3` running | 18,128 B free | **74,720 B free** |

The +65,056 at idle is the +65,536 region minus the second region's allocator
bookkeeping. ⚠️ The `quad-strips-v3` row is **not** a clean +/- comparison:
`used` came out ~9 KB above the 94,508 B recorded earlier for that project, for
reasons unrelated to this change (which only *adds* heap). Quote the idle row
for the reclaim figure.

This does not cost `.stack` anything — that is the point. The 64 KiB comes from
a segment `.stack` never had access to, so the "as high as it links" ceiling
above is untouched.

**What it gives up, stated plainly.** The old 92 KiB could hold the entire
27-shader corpus at once (90,400 B); 32 KiB cannot. Shaders emitting between
32,768 B and 94,208 B used to be placeable and no longer are — roughly 17–50 KB
of GLSL, against a largest real shader of 4 KB. That range is already
unreachable on this chip for a different reason: a 4 KB shader needs ~65 KB of
compile working set (below), so a 17 KB one cannot compile at any region size.
The code region was never the binding constraint for those shaders; the heap
was, and this trade moves 64 KiB to the side that binds.

⚠️ **That last argument is the one thing here with a moving dependency.** It
rests on the ~65 KB compile working set, and PR #284 shrinks that figure:
`ChunkedVec` was bounding chunks in *elements* (64), so a 96-byte `HirExpr`
gave 6,144 B chunks with a 9,216 B peak across the doubling step; at
`CHUNK_BYTES = 1024` those become 960 B and 1,440 B. If the transient falls far
enough that a 17 KB-GLSL shader becomes compilable, then for *that* shader the
32 KiB region really would be the binding constraint, and this paragraph would
need revisiting rather than merely re-baselining.

Two reasons not to treat that as urgent: the corpus guard in
`tests/xt_classic_codemem_corpus.rs` fails on the host the moment a real shader
outgrows the region, so the failure mode is a red CI rather than a surprise on
a board; and the reclaim's *value* argument only strengthens as the transient
shrinks (less transient plus more heap). Re-derive the ~65 KB with #284 in
before quoting it again.

Verified on silicon: the JIT arena's span accounting closes exactly across a
project swap (`allocs=2 frees=1 spans=1 used=2032`, `largest_free` back to
`32768 − 2032` unfragmented), so spans are returned, not leaked.

The `TooLarge` backstop is verified **on the host**, at real-region scale: the
refusal leaves the region whole and the next allocation succeeds
(`tests/xt_classic_codemem_corpus.rs`).

⚠️ On the device it was **not** observed, and the reason is worth recording. A
deliberately oversized shader (26 KB of GLSL, 48,152 B of Xtensa) never reached
placement: the compile *crashed* twice and `lp-recovery` disabled the
shader-compile frame, leaving the node on its black fallback while the board
stayed up at full frame rate. That is the graceful outcome, but it is the
**heap** limit failing first, not the region limit — more evidence that the
code region was never what bound large shaders on this chip. A device-side
`TooLarge` would need a shader that is large in *emitted code* while cheap to
compile, which nothing realistic is; the `[JIT] fails=` counter added here is
where it would show up if one ever appeared.

### Runtime heap, measured on silicon (112,640 B arena)

> These rows predate the 2026-08-02 amendment above and were taken against the
> single 112,640 B arena. Add 65,536 B to every `free` figure to reach today's
> image; `used` is unchanged.

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
`dither_overflow`. The other **~68 B/LED is engine-side and scales with
`render_size`**; it is unattributed and is the single most valuable RAM lead
this chip has.

**Practical ceiling: ~240 LEDs comfortable, ~300 at the edge, 400 impossible
— for a project whose shader is already compiled.**

> **Amended 2026-08-02 — the ceiling moves up with the reclaimed 64 KiB.** At
> ≈89.5 B per LED, 65,536 B of new heap is **≈730 LEDs** of headroom on the
> steady-state axis, so the 400-LED row that OOM'd is now expected to fit with
> room, and the steady-state ceiling is no longer the interesting limit.
>
> ⚠️ These are *derived*, not measured — the arithmetic above the amendment
> stands, but nobody has run 400 LEDs on the two-region image yet. Do not quote
> a new LED number for a product claim until someone does; measure and replace
> this paragraph.
>
> The more important consequence is the **other** axis. The binding constraint
> was never LED count alone but LED count × shader size, via the ~65 KB compile
> working set — and 64 KiB of new heap lands squarely on it. Whether
> `examples/basic` (241 LEDs, 4,092 B GLSL) now compiles on-device, where it
> previously OOM'd, is the single measurement most worth taking next.

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

### Radio, if it is ever attempted

M2-P3 measured `wifi::new` + STA at **44,244 B of heap** plus ~28 KB static
and ~390 KB flash. Against an app that already uses ~94 KB of a 110 KB arena,
WiFi + JIT does not fit without a RAM diet of serde-surface scale. That is the
measured price of D1's "radio-off for v1", recorded here so the future attempt
starts from a number rather than a hope.

> **Amended 2026-08-02.** The 44,244 B heap requirement now has somewhere to
> come from: the reclaimed 64 KiB exceeds it. `quad-strips-v3` running leaves
> 74,720 B free, against 44,244 B needed — so the *heap* half of the radio
> question has flipped from "impossible" to "arithmetically available", which
> it has not been before.
>
> ⚠️ This does **not** reopen D1, and nobody should read it as radio being
> affordable. Three things are unaddressed: the ~28 KB of radio *static* data
> comes out of `dram_seg`, which is still zero-sum with `.stack` and is the
> segment that actually binds; the radio-probe build already has to shrink the
> arena to 72 KB just to link; and no measurement has been taken with both the
> radio and a project resident. The honest statement is that the heap objection
> is no longer the *first* blocker — the `.bss`/`.stack` one now is.

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
- **The JIT code region is settled at 32 KiB and should not be re-litigated
  from arithmetic.** It is pinned by two guards that will speak up on their
  own: `tests/xt_classic_codemem_corpus.rs` fails if a real shader outgrows it,
  and the const-asserts in `codemem_esp32` fail if the region and the heap
  boundary stop abutting. Changing it means changing `lp-xt-emu`'s
  `BoardProfile::esp32()` in the same commit — `tests/xt_classic_profile.rs`
  enforces that.
- `largest_free_block()` no longer measures fragmentation on its own: with two
  heap regions, `free − largest` conflates fragmentation with a cross-region
  split. Use `esp_alloc::HEAP.stats()` for a per-region breakdown before
  concluding a heap is fragmented. This matters for anyone reading the
  retry-succeeds-OOM signature.

## Reproducing

```bash
just fw-esp32v3-size-check
cd lp-fw/fw-esp32v3 && cargo bloat --profile release-esp32v3 --crates -n 18
```

Per-variant images: `touch src/main.rs` before each build (feature-driven cfg
is not reliably tracked on this crate), then `espflash save-image --chip esp32
--flash-size 4mb <elf> <out>`. Section sizes via `xtensa-esp32-elf-size -A`.
Runtime heap comes from the device's own heartbeat `memory` field.
