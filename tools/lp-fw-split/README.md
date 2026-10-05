# lp-fw-split

Builds the ESP32-C6's **split image**: one firmware link, split by
reachability into a **core** (boot, radios, links, the filesystem, the boot
bookkeeping) and an **engine** (everything else — the server, the node graph,
the GLSL compiler), plus the RAM-only loader that boots the core and the two
boot records that say which core to boot.

```text
lp-fw-split build --out target/fw-split/<slug> [--features esp32c6,server]
lp-fw-split reach  <elf> <map> --emit-ld <engine.x>   # one link → placement rules
lp-fw-split verify <elf> <map>                        # one (pass-2) link → verdict
```

`lp-cli firmware build|package` use the library; the just recipes use the
binary. It is a host tool (`std`) with no Python and no binutils: the ELF and
the map are read from the ELF32 specification and lld's map format, images
are made by espflash 3.3's library, and the boot-record format and layout
come from `lp-bootctl` itself.

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
8. **Loader.** Built from its own directory; its ESP image by espflash. The
   build refuses a loader image without this tree's version word
   (`lp_bootctl::loader_identity`) or longer than `LOADER_MAX_LEN`.
9. **`app.bin`** for `0x10000`: loader, record sector 0 (seq 1, proven),
   record sector 1 erased, core at `0x18000`, engine at the first 32 KiB page
   after the core — all through `lp-bootctl` — then `0xFF` to the next 4 KiB
   flash sector (`image_end`): no image this tool makes ends mid-word.
10. **`merged.bin`**: the whole 4 MiB chip — espflash 3.3.0's bundled
    bootloader (or a build def's override), the partition table, `app.bin` —
    with the flash settings `scripts/emu/build-merged-image.sh` uses (DIO,
    40 MHz, 4 MB).
11. **`split.json`**: offsets, lengths and SHA-256s of every piece.

Outputs: `p1.elf p1.map engine.x p2.elf p2.map verify.txt loader.elf
loader.bin core.bin engine.bin app.bin merged.bin split.json`.

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
bootloader selects on a 4 MB C6 (`app_image::PAGE`). The loader and the core
read the real page from the MMU and refuse a mismatch; the ROM-up emulator
gate boots the real bootloader, so a bootloader that chose another page
fails CI.
