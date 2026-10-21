# lp-fw-split

Builds a **split image**: one firmware link, split by reachability into a
**core** (boot, radios, links, the filesystem, the boot bookkeeping) and an
**engine** (everything else — the server, the node graph, the GLSL compiler),
plus the RAM-only loader that boots the core and the two boot records that
say which core to boot. The ESP32-C6's is the product image; the ESP32-S3's
(`--chip esp32s3`, the S3 roadmap's M7) takes the same shape.

```text
lp-fw-split build --out target/fw-split/<slug> [--chip esp32c6|esp32s3] [--features …]
lp-fw-split reach  <elf> <map> --emit-ld <engine.x> [--chip …]  # one link → placement rules
lp-fw-split verify <elf> <map> [--chip …]                       # one (pass-2) link → verdict
lp-fw-split why    <elf> <map> <substring> [--chip …]           # how the core reaches a section
lp-fw-split segments <core.bin>... [--chip …]                   # an image's segments; fails on one the loader cannot place
lp-fw-split headroom <out>/split.json [--margin 65536]
```

`lp-cli firmware build|package` use the library; the just recipes use the
binary. It is a host tool (`std`) with no Python and no binutils: the ELF and
the map are read from the ELF32 specification and the linker's own map
format, images are made by espflash 3.3's library, and the boot-record
format and layout come from `lp-bootctl` itself.

## Chip profiles

Every step reads its chip-specific facts from a `ChipProfile`
(`src/chip_profile.rs`). The C6's reproduces the pipeline as it shipped,
byte for byte (the profile refactor was checked by building the C6 with the
tool before and after on one tree: identical `engine.x`, `core.bin`,
`engine.bin`, `app.bin`, `loader.bin`, `bootloader.bin` and merged images).
The S3's is the C6's shape with only what the chip forces changed:

| | ESP32-C6 | ESP32-S3 |
|---|---|---|
| firmware, target | `lp-fw/fw-esp32c6`, `riscv32imac-unknown-none-elf` | `lp-fw/fw-esp32s3`, `xtensa-esp32s3-none-elf` |
| linker, map | `rust-lld`, lld's map | GNU ld through `xtensa-esp32s3-elf-gcc` (`-Wl,…`; the espup toolchain's binutils when not on `PATH`), GNU ld's map (`gnu_map`) |
| relocation targets | `S + A` | **from the linked bytes** (`xtensa_targets`, below) |
| engine | one window at `0x4240_0000` | rodata at `0x3D00_0000` (data view), code at the instruction view of the next byte (`0x4300_xxxx`): one run of flash pages, MMU entries `0x100..`, both views |
| literal pools | — | each engine function's `.literal.X` placed right before its `.text.X` (`literal_pools`) |
| page, layout | 32 KiB, layout 1 (region at `factory + 0x8000`) | 64 KiB, layout 2 (region at `factory + 0x1_0000`) |
| loader | `lp-fw/fw-esp32c6-loader`, factory + our second stage | `lp-fw/fw-esp32s3-loader`, factory only |
| doors | ours (default) and the IDF's | the IDF's (espflash's bundled S3 bootloader) |
| seam table | required | none yet (Xtensa seams are the S3 roadmap's M5) |

### What GNU ld on Xtensa does that lld on RV32 does not

Found by the split-image spike (2026-10-20) on GNU ld's output — never read
off its source (AGENTS.md, license rule):

1. **Stale addends.** `--emit-relocs` keeps a zero addend on relocations
   against a merged section's symbol: 18,355 of 30,236 `R_XTENSA_32` read
   `<section> + 0` while the word they patch points well inside it. `S + A`
   aimed half the data edges at the first section of `.rodata`, and the
   first spike run "passed" with 389 core sections in the engine. The S3
   reader takes an `R_XTENSA_32`'s target from the word at `r_offset`.
2. **Literal coalescing.** Equal literals of different functions share one
   pool slot, so a function's `l32r` can load from another function's pool.
   An `l32r` is therefore an edge to the address **its slot holds** (when the
   slot carries an `R_XTENSA_32`; a constant is no edge), never to the pool:
   the spike's second run made the whole engine core through
   `lp_engine_entry`'s pool. Code reaches its own pool one way, so a core
   function's pool is core.
3. **`l32r` reaches only 256 KiB backwards**, so the engine script writes
   every engine function's pool rule right before its code rule. A pool that
   relaxation emptied in pass 1 is no node there, but pass 2 starts from the
   object's own pools again, so the rule is written regardless.
4. **Every call is `l32r` + `callx8`** (relaxation narrows the in-range ones
   to `call8`, whose target is decoded): a call has no range, so engine →
   core calls across 16 MiB link.

## The pipeline

1. **Pass 1.** `cargo rustc` in `lp-fw/fw-esp32c6` with `LP_SPLIT_LINK=1`
   (the `lp_split` cfg), `--emit-relocs`, `-Map`, and a script that places
   only the engine header at `0x4240_0000`.
2. **Reach.** Nodes are the map's input sections; edges are the relocations.
   The core is everything reachable from the reset entry, the app
   descriptor and every RAM-resident code section. The engine header is not
   a root and the core never names it (it reads it as data), so the walk
   never crosses into the engine. Everything else is engine.
3. **`engine.x`.** Every engine input section from a Rust object, in address
   order, into `.engine_rodata`/`.engine_text` at `0x4240_0000`, header first.
4. **Pass 2.** The same link under `engine.x`.
5. **Tree guard.** `git rev-parse HEAD` and a hash of `git status
   --porcelain`, before pass 1 and after pass 2. `build.rs` bakes the commit,
   the dirty flag and the version into the image, so a commit or an edit
   between the passes would link two programs. The version is resolved once
   and handed to both passes, because a dirty tree's dev version carries the
   time.
6. **Verify** (pass 2): **0 core nodes in the engine region**, or the build
   fails naming them. `verify.txt` says so on its first line.
7. **Split.** `engine.bin` is the two engine sections' bytes from
   `0x4240_0000`. The core's ESP image is espflash's own image of the pass-2
   ELF with the engine's two section headers re-typed `NOBITS`.
   **Then the patches, in this order** (all through `lp-bootctl`):
   1. the engine header's `len` and `crc` are filled, after checking it is a
      committed v1 header whose build id equals the core's own copy
      (`LP_BUILD_ID`);
   2. SHA-256 of `engine.bin` — exactly as flashed, header patched and
      committed — is written into the core's digest slot
      (`LP_ENGINE_DIGEST`, found by symbol; both statics are extra core
      roots, so they are never placed in the engine);
   3. only then is the core's ESP image made, so its checksum and appended
      hash cover the patched slot. The tool checks the slot equals the
      engine's SHA-256 and refuses the build otherwise.

   **Every segment of `core.bin` must be one the loader can place**
   (`core_segments`): inside the core's flash (`0x4200_0000` to the engine
   region, at its page offset), or inside the core's RAM (esp-hal's C6 `RAM`
   region) less the loader's own. The loader copies any other segment to its
   address, and espflash 3.3.0 writes an address-0 padding segment when a
   page crossing leaves a gap the RAM segments cannot fill: on silicon that
   copy faulted before the core ran
   (`docs/defects/2026-10-14-the-c6-loader-copies-an-address-zero-padding-segment-to-address-zero.md`).
   So first each such padding segment's load address becomes a run at the
   start of the core's `.bss` (`core_padding`; `_bss_start.._bss_end` from
   the pass-2 ELF). Its bytes stay where they are, so every later segment
   keeps its offset, and the loader copies zeros that riscv-rt zeroes again
   before Rust runs. The appended SHA-256 is recomputed. Then any segment the
   loader still cannot place fails the build, and `lp-cli firmware package`
   checks the bytes it ships the same way.
8. **Loader.** Built from its own directory; its ESP image by espflash. The
   build refuses a loader image without this tree's version word
   (`lp_bootctl::loader_identity`) or longer than `LOADER_MAX_LEN`.
   **Then our second stage**: the same crate with `--features bootloader`
   (`bootloader.x`), whose image `bootloader_image` writes itself — header
   (DIO, 40 MHz, 4 MB, chip id `0x000D`), RAM segments, XOR checksum,
   appended SHA-256 — and checks the way the ROM reads it, refusing a stage
   without this tree's `LPLB` word or one that would run into the partition
   table. `bootloader.elf`, `bootloader.bin`. See
   [`docs/adr/2026-10-17-the-c6-boots-through-our-own-bootloader.md`](../../docs/adr/2026-10-17-the-c6-boots-through-our-own-bootloader.md).
9. **`app.bin`** for `0x10000`: loader, record sector 0 (seq 1, proven),
   record sector 1 erased, core at `0x18000`, engine at the first 32 KiB page
   after the core — all through `lp-bootctl` — then `0xFF` to the next 4 KiB
   flash sector (`image_end`): no image this tool makes ends mid-word.
10. **The merged images**: the whole 4 MiB chip, the partition table and
    `app.bin` behind each front door. `merged-idf.bin`: espflash 3.3.0's
    bundled IDF bootloader (or `--bootloader`'s) with the flash settings
    `scripts/emu/build-merged-image.sh` uses (DIO, 40 MHz, 4 MB), which
    starts the factory loader. `merged-lp.bin`: the same bytes with
    `bootloader.bin` at `0x0` and `0xFF` to `0x8000` — the two differ in
    `0x0..0x8000` only. **`merged.bin`** is the build's door (`--door
    lightplayer`, the default, or `--door idf`); packaging, Studio and the
    emulator gates read it. `lp-fw-split bootloader <out>` rebuilds only the
    second stage into an existing build.
11. **`split.json`**: offsets, lengths and SHA-256s of every piece.

Outputs: `p1.elf p1.map engine.x p2.elf p2.map verify.txt loader.elf
loader.bin bootloader.elf bootloader.bin core.bin engine.bin app.bin
merged-idf.bin merged-lp.bin merged.bin split.json` (the S3 has no
`bootloader.*` or `merged-lp.bin`: its only door is the IDF's).

The steps above are the C6's words; the S3 takes each one with its profile's
facts (its region at `0x2_0000`, 64 KiB pages, an 8 MiB chip, espflash's
bundled IDF bootloader). `split.json` names the chip and the region start.

## Why placement is by reachability (the spike's six traps)

The split-link spike (2026-10-01) hit six traps; each is now built in:

1. **Inlining makes crate-level reasoning wrong.** `init_board`, the BLE
   controller init and the lpfs mount are inlined into the boot function.
   Placement is by reachability, never by crate.
2. **LLVM merges identical tiny functions** under an arbitrary crate's name
   (every no-op `Debug::fmt` under `fmt-debug=none`). Reachability places
   the merged copy correctly; a crate rule would not.
3. **Section names are not unique across objects** (`compiler_builtins`,
   never LTO'd, has its own `OUTLINED_FUNCTION_N`). A rule names the object
   and the section.
4. **The object's file name carries a hash that changes with the link
   arguments,** so rules glob it.
5. **Where the script is inserted matters** (`INSERT BEFORE .rodata`): the
   app descriptor once followed the engine into its region.
6. **`--emit-relocs` keeps `.rela.*` sections** that objcopy refused to drop
   the engine sections around. This tool never runs objcopy: it re-types the
   two section headers instead, and espflash takes its segments from
   `PROGBITS` sections only.

## When the core is bigger than it should be

`lp-fw-split why <p1.elf> <p1.map> <substring> [--chip …]` prints the chain
of input sections by which the core's roots reach the first section whose
`object:(section)` contains the substring — the question to ask when
something of the engine (`lp_engine_entry`, `LpServer`, `lps_glsl`) shows
up in the core. A chain through shared generic code (`RawVec::grow_one`
folded across types) is expected; a chain through a literal pool or a
section's first node is a reader bug.

## Reading `verify.txt`

- The first line is the verdict: `core nodes in engine region: 0` passes.
  Anything else fails the build and lists the nodes.
- `engine nodes left in core region` is expected: C archive members only the
  engine reaches stay where the link put them (a few KB; the core never calls
  them).
- `edges core -> engine` is expected to be **0**: the core reaches the engine
  only through the header, which it reads as data.
- The two crate tables are the evidence of what went where. The shader
  compiler (`lps_glsl`, `lpvm_native`) and the engine (`lpc_engine`) are in
  the engine's table.

## The page

The layout assumes a **32 KiB** MMU page, what espflash 3.3.0's bundled IDF
bootloader selects on a 4 MB C6 and what our second stage sets explicitly
(`app_image::PAGE`). The loader and the core
read the real page from the MMU and refuse a mismatch; the ROM-up emulator
gate boots the real bootloader, so a bootloader that chose another page
fails CI.
