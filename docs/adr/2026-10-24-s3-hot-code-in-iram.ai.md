# ADR: The S3 links its hottest ~40 KB of flash code into IRAM, chosen by a silicon profile

- **Status:** Accepted
- **Date:** 2026-10-24
- **Deciders:** Photomancer (Yona approved the sequence in chat on
  2026-10-24, "start these in your suggested order")
- **Supersedes:** None
- **Superseded by:** None
- **Plan:** `lp2025/2026-10-20-2335-s3-radio-and-ota` (the fps lane, step 1;
  the evidence is the fps spike, `fps-spike/report.md` there, branch
  `claude/s3-fps-spike-350920`)

## Context

The fps spike (2026-10-23, FS3) found that the S3's non-shader frame time is
per lane, not per LED: about 4.7 ms of engine graph walk per output lane per
frame, ~90 k instructions running at ~12.5 cycles each. The cause is
instruction fetch. That code runs from flash through a 32 KB instruction
cache (already the S3's maximum), and its footprint is far larger, so it
streams: halving the cache added 22 % to it, and a near-free shader left it
unchanged. The shader is not evicting it; it misses on its own.

The spike moved the 40 KB of functions its PC sampler saw most, per byte,
into `.rwtext` through esp-hal's `ESP_HAL_CONFIG_USE_RWTEXT_LD_HOOK` and
measured +27–38 % fps. It was the largest single lever it found, ahead of
the second core.

## Decision

1. **The hot set is linked into `.rwtext`** on every `fw-esp32s3` build:
   `.cargo/config.toml` sets the hook, and esp-hal INCLUDEs
   `lp-fw/fw-esp32s3/rwtext_hook.x`.
2. **The list is generated from a committed silicon profile, never edited
   by hand.** `lp-fw/fw-esp32s3/hot_iram/profile.tsv` holds per function
   the samples, the size at capture, the demangled name and a linker pattern.
   `just s3-hot-iram-gen` writes the hook from it, greedy by samples per
   byte under 40,960 B. The same profile always writes the same hook, so an
   unrelated change never moves it.
3. **Patterns survive a rebuild.** A pattern is the function's section name
   with the parts of a Rust v0 symbol that change between builds globbed:
   the crate disambiguator, back-references, and an LTO `.llvm.<n>` suffix,
   plus the `.text.unlikely.` spelling LLVM uses for code it thinks cold.
   The first profile, taken on the spike's branch, matched all 191 of its
   functions on `main` a day later.
4. **A check keeps it honest.** `just s3-hot-iram-check` runs inside
   `just fw-esp32s3-size-check` (CI's S3 size job). It fails when nothing
   landed in IRAM, when the hot set with its literal pools outgrows the
   arena's reservation, or when the functions it names have vanished from
   the image carrying more than 10 % of its samples. The way back is in
   `hot_iram/README.md`: rename a row, regenerate, or re-profile on FS3.
5. **The profile is taken with the `pc-sampler` image.** Core 1 reads core
   0's PC from the debug assistant's pdebug record register
   (`CORE_0_RCD_PDEBUGPC`) at random gaps, from IRAM. It costs core 0
   nothing and is not biased by core 0 masking interrupts. It is a
   diagnostic feature, never shipped.
6. **The arena pays.** `.rwtext` comes out of `dram_seg`, zero-sum with
   `.stack`, so the arena shrinks by `HOT_IRAM_BYTES` (43 KiB; `.rwtext`
   grew by 43,296 B) and the main stack keeps its share.
7. **On the split image the hot code rides in the core's RAM segment**,
   which the loader copies in at boot, and it is **not a core root**
   (`lp-fw-split`'s `Node::is_hot_ram_code`: a Rust function's own
   `.text.*` that a linker rule moved into RAM code). An engine function in
   IRAM stays engine by reachability. It may call engine code, and the core
   never calls it. Rooting it would have pulled most of the engine into the
   core. Core and engine always come from one link (the core's digest slot
   names its engine), so the engine code in the core's RAM always matches
   the engine that calls it. `#[ram]` code (`.rwtext.*`) and the radio
   blobs' IRAM are roots as before, and the C6, which has no hook, is
   unchanged.

## Measured (FS3, XIAO ESP32-S3 Plus 16 MB, antenna attached, BLE advertising, no central)

The shipped split image, release `2026.10.24-1`, against this branch's split
image, both from reset with a console. fps is frames per second over three
5 s `[perf]` windows after the first.

| project (lanes × LEDs) | before | after | |
|---|---:|---:|---:|
| `skirt-620` (2 × 310) | 32.0 | 41.2 | +29 % |
| `n6x248` (6 × 248 = 1,488) | 14.6 | 20.6 | +41 % |
| `n8x256` (8 × 256 = 2,048) | 11.1 | 15.8 | +42 % |

Internal RAM free at boot (`[MEM]`): 187,580 → 143,548 B (−44,032); largest
free block 107,576 → 73,744 B (now `dram2_seg`'s untagged region; the
tagged arena is 92,160 B). PSRAM holds Rust's allocations (policy `all`),
so the arena serves DMA, the radio's internal requests and thread stacks.

Split image (`just fw-esp32s3-size-check`): core 1,042,336 → 1,080,960 B,
engine 2,126,801 → 2,088,945 B, `app.bin` 3,244,032 → 3,268,608 B, steady
headroom 3,050,543 → 3,022,863 B. The core grows by the engine code it now
carries in RAM. The C6 is untouched: no C6 file changes, and the split
tool's new rule matches nothing in a C6 link (no hook).

## Alternatives considered

- **`dram2_seg` (the 72 KiB the second-stage bootloader ran in) instead of
  the arena.** Code there cannot be loaded by the bootloader, which is
  running in it, and the IDF bootloader refuses a segment that overlaps its
  own memory. It would need a flash copy (LMA in flash, VMA in `dram2_seg`'s
  I-bus alias) that every image path writes and loads: espflash's plain
  image, `lp-fw-split`'s core, the emulator's direct load and ROM-up boot. And
  `dram2_seg` is already a heap region (`dram2_heap`, untagged), so the
  internal-free figure would drop by the same 43 KiB either way. Only the
  kind of byte would differ: tagged arena or untagged region. Not practical
  for that.
- **A hand-maintained list** (the spike's). It rots on the first rename, and
  nobody can say whether it is still right.
- **Instruction-count profiles from the emulator** (`lp-emu:esp32s3:t1`
  prices every instruction at one cycle). They rank dense shader loops,
  already cache-resident, above the graph walk that actually misses. The
  spike measured t1 misranking a related question 2.5×. The sampler measures
  where silicon's cycles go.
- **A 64 KB hot set** covers 80 % of flash-resident samples against 69 %.
  It did not fit beside the LCD's internal DMA buffer in the spike, and the
  arena is what pays.

## Consequences

- An engine PR can make the hot set stale. The check fails it when more than
  10 % of the hot set's samples are gone, and says which functions. A rename
  is a one-line profile edit.
- A future engine restructure (the static frame plan) changes what is hot.
  Re-profile then, with the README's runbook.
- The S3 core image carries ~38 KB more in RAM segments, and the loader copies
  it at every boot.
