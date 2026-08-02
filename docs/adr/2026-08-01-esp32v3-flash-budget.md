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
bookkeeping. **Quote the idle row for the reclaim figure** — it is the only
clean +/- comparison here.

⚠️ **The loaded row is not, and the reason is worth recording: the ADR's own
94,508 B figure is stale relative to `main`.** Three measurements of
`quad-strips-v3` loaded, `used` bytes:

| measurement | arena | used |
|---|---|---|
| the 94,508 B row above (older `main`) | 112,640 B | 94,508 B |
| PR #285's, on `main` @ `e2272d0f8` | 112,640 B | 98,244 B |
| this change | 178,176 B | 103,456 B |

The middle row is the one that settles it: it is +3,736 B over the recorded
figure on the **unchanged** arena, taken with a change that only *subtracts*
heap use. So the loaded cost has grown in `main` independently of both PRs, and
the ~9 KB seen here is that drift measured from a newer `main` (plus whatever a
different fragmentation regime contributes at 178,176 B) — not a cost of the
reclaim, which only adds heap and cannot raise `used`.

Recorded as three regimes rather than absorbed into either PR's narrative. It
deserves a bisect of its own; neither PR owns it, and `used` figures taken
before and after this change are not comparable in any case, because they are
measuring different heaps.

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

**That argument was checked rather than assumed, and it holds with room.** It
rested on the ~65 KB compile working set, which PR #284 shrinks — so the
question was whether a 17 KB-GLSL shader might become compilable, in which case
the 32 KiB region *would* genuinely bind for it. Measured on the host
(`spikes/glsl-compile-working-set`, counting a `#[global_allocator]` over a real
`lps_glsl::compile`), peak heap scales linearly with source at ~38 B per byte
of GLSL:

| GLSL | peak heap | largest single allocation |
|---|---|---|
| 4,092 B (`examples/basic`) | 156,972 B | 24,576 B |
| 17,714 B (synthetic) | 1,680,167 B | **196,608 B** |

The decisive figure needs no host-vs-device caveat: at 17 KB of GLSL the
**single largest allocation alone is 196,608 B**, which exceeds even the
two-region 178,176 B heap — before counting anything else. A 17 KB shader
cannot be *lexed* on this chip, let alone compiled, at any region size. The
paragraph above is if anything understated, and #284 does not move it, because
the dominant term was never `ChunkedVec`.

### ⚠️ The compiler's largest single allocation is the lexer's token vector

Measured in the same pass, and the more useful finding: `lps_glsl::lex` alone
accounts for the whole 24,576 B peak allocation — a plain doubling
`Vec<Token>`, not chunked at all. `Token` is **12 bytes on `riscv32imac` and
on the 64-bit host alike**, so unlike the peak-heap figures this transfers to
the device unchanged.

That means compiling `examples/basic` asks the classic's allocator for a single
**24,576 B contiguous block — 22 % of the old 112,640 B arena, and 8× the
3,072 B request whose failure was originally diagnosed as the OOM**. The
`ChunkedVec` backtrace named what happened to fail, not what was largest.

This does not change the reclaim argument, but it is the allocation the
reclaimed 64 KiB actually has to accommodate, and it is a *contiguous* one — so
it is also the case where "two regions cannot serve one allocation spanning
both" bites. Chunking the token vector, or lexing on demand, is the next RAM
lever on this chip.

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
| `quad60-v3` (4 × 60 = 240 LEDs) | **7,384 B** ⚠️ stale — OOMs on `main` today | 105,256 B |
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

> **Amended 2026-08-02 — 240 LEDs went from impossible to working, measured.**
>
> This is no longer arithmetic. `quad60-v3` (4 × 60 = 240 LEDs) **OOMs on
> current `main`**: two boots, two allocation failures inside the shader node,
> then recovery disables the node and the board sits at `level=red`. It never
> reaches the shader compile. The second failure reports `free=548
> used=112,092` of the 112,640 B arena — genuinely exhausted, not fragmented.
> (Measured with PR #285's 13 B/LED reduction already applied; it OOMs anyway.)
>
> On the two-region image the same project, from the same startup state, runs:
>
> | | `main` (112,640 B) | reclaimed (178,176 B) |
> |---|---|---|
> | outcome | **OOM ×2 → node disabled, `level=red`** | runs, `level=green` |
> | `used` | 112,092 (exhausted) | 110,588 |
> | `free` | 548 | **67,588** |
> | shader | never compiled | compiled, `[JIT] used=2032 fails=0` |
>
> Power-on boot, `bootCount=1`, `safeMode=false`, 14.3 fps, 201 s soak with
> zero OOM or crash lines and the heap flat to within 4 B.
>
> **The decisive figure is the transient, not the steady state.** While this
> project loads — bringing up four WS281x RMT channels for 240 LEDs — peak
> `[MEM] used` reaches **113,968 B**, which exceeds the *entire* 112,640 B
> arena that preceded this change. (The `[MEM]` line carrying it is stamped
> `[JIT] used=2032`, confirming it belongs to this project rather than to a
> larger shader compiling alongside.) That is not evidence the old heap was
> merely tight: a heap that never had 113,968 B could not have served this
> load under any allocator, independent of fragmentation, region layout or
> free-list shape. The steady state settles back to `used=110,588`.
>
> ⚠️ **The `quad60-v3 → 7,384 B free` row in the table above is therefore
> stale** — it is not currently reproducible on `main`, in the same direction
> as the loaded-`used` drift recorded earlier. That is the row a 240-LED
> product claim rests on, so treat this amendment as replacing it.
>
> The wider ceiling is still *derived*: at ≈89.5 B/LED, 65,536 B is ≈730 LEDs
> of headroom, which suggests the 400-LED row should now fit. **Nobody has run
> 400.** Do not quote a number above 240 for a product claim until someone
> does.
>
> ⚠️ The division also assumes the per-LED allocations are individually small
> enough to land in whichever region has room. **A second region cannot serve a
> single allocation spanning both**, so any one contiguous buffer that scales
> with LED count is bounded by the larger region, not by the 178,176 B total.
> `largest_free` is the figure to watch, not `free`. (The compiler already has
> such an allocation — the 24,576 B token vector above.) PR #285's per-LED
> attribution is the place to check whether any per-LED cost is one big buffer
> rather than many small ones; if it is, this arithmetic does not apply.
>
> **The other axis moved too, and this one is measured as well.** The binding
> constraint was never LED count alone but LED count × shader size, via the
> compile working set. `examples/basic` (4,092 B of GLSL) is the shader this
> ADR recorded as OOMing on-device. **It now compiles**, in 188 ms:
>
> ```
> [shader-node] compilation succeeded (node=NodeId(4), elapsed=188ms,
>   lpir_inst_count=586, lpir_func_count=12, lpir_import_count=7,
>   final_inst_count=1629, final_code_size=6516 bytes, float=fixed)
> ```
>
> `final_code_size=6516` and `lpir_func_count=12` are exactly what
> `tests/xt_classic_codemem_corpus.rs` predicts for this shader — the host
> instrument reproducing the device a third time.
>
> **Independently reproduced** on a second flash by the session working PR
> #285: `/projects/Basic` auto-loaded and ran, compiling in 183 ms with
> `[JIT] used=6516` and `[MEM] free=110,764 used=67,412`. Two sessions, two
> boards states, the same result — and `[JIT] used=6516` matching the corpus
> prediction to the byte on both.
>
> ⚠️ Scope of the claim, twice narrowed. First: in this session's run the
> deploy did not report the project as running afterwards (the separate
> `lp-cli upload` acks-but-never-activates defect), so *this session* measured
> only that the shader compiles; the #285 session's run is what shows it also
> **runs**. Second, and a correction to an earlier draft of this ADR: the
> 113,968 B peak recorded below belongs to the **240-LED project's load**, not
> to this compile — the `[MEM]` line carrying it is stamped `[JIT] used=2032`,
> which is `quad-strips`-sized code, not this shader's 6,516 B. A clean
> single-project compile of `examples/basic` settles at `used=67,412`, which
> would have fit the old arena; what is established here is that the shader
> **now compiles where the ADR recorded it OOMing**, not that its own peak
> exceeds 112,640 B.

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
