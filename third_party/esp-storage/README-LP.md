# esp-storage — LP fork

Vendored in `cea343c4eb` (2026-05-20) from **esp-hal's `main`**, not from a
crates.io release: the `esp-storage/` directory as it stood between esp-hal
`75eaba1a` (2026-05-07) and `16117ae0` (2026-05-28), which have identical
`esp-storage/` trees. The version stays `0.9.0` on purpose: the three
firmwares (`fw-esp32c6`, `fw-esp32s3`, `fw-esp32v3`) depend on `esp-storage`
by version, and the root `Cargo.toml`'s `[patch.crates-io]` only substitutes a
source, never a version. MIT OR Apache-2.0, as upstream.

## What it carries that crates.io 0.9.0 does not (upstream's, not ours)

Because it was taken from `main`, the fork already has upstream's post-0.9.0
changes. The one that changes behaviour is esp-hal #5429 ("Don't rely on the
ESP-IDF bootloader to know the flash size", `00bb4f4c`): `FlashStorage::new`
asks the flash chip (RDID, `get_flash_size` in `src/hardware.rs`; the ROM's
`g_rom_flashchip` on the classic ESP32) instead of reading the size byte of an
image header at `0x0`/`0x1000`/`0x2000`. The rest is lint-level: `is_multiple_of`
for `%`, a let-chain, a new logo URL.

## Our diff: `panic-unaligned-buffer`

A new cargo feature, off by default, which every firmware turns on.

Upstream's `NorFlash::read` and `write` take a caller's buffer that is not
4-byte aligned through a **4 KB `FlashSectorBuffer` on the stack**
(`FlashSectorBuffer::uninit()`), and copy through it a sector at a time. On a
thread with a few KB of stack, one unaligned call is a stack overflow. With
the feature on, that path is a panic with a sentence instead
("esp-storage read called with unaligned output buffer", and the same for
write), so the sector buffer is never linked. Our callers keep their buffers
aligned (the C6's tree store writes through
`fw_esp32_common::aligned_nor_flash`).

The patch:

- `Cargo.toml`: the feature, and the `path = "../…"` entries dropped from
  `esp-hal-procmacros`, `esp-hal`, `esp-sync`, `esp-rom-sys` and
  `esp-metadata-generated` (they resolve from crates.io, or from the root's
  own patches).
- `src/nor_flash.rs`: `read` and `write` split into `read_word_aligned` /
  `read_unaligned` and `write_word_aligned` / `write_unaligned`. The aligned
  halves are upstream's loops, moved. The unaligned halves are `#[cold]`,
  `#[inline(never)]`, upstream's loops without the feature and a `panic!` with
  it. The `FlashSectorBuffer` and `uninit_slice` imports are behind
  `not(feature = "panic-unaligned-buffer")`.
- `src/buffer.rs`: `uninit_slice` and `FlashBuffer::assume_init_bytes` behind
  the same `cfg`; `FlashWordBuffer` gains
  `#[cfg_attr(not(feature = "bytewise-read"), expect(unused))]`.

`grep -rn 'panic-unaligned-buffer' src Cargo.toml` finds every site.

## Re-syncing with upstream

Copy `esp-storage/` out of the esp-hal release (or commit) you want, drop the
`path = "../…"` entries from its `Cargo.toml`, then re-apply the feature
above. Diff against the base named at the top to see exactly what is ours:

```bash
git -C <esp-hal checkout> archive 16117ae0~1 esp-storage | tar -x -C /tmp/up
diff -ru /tmp/up/esp-storage third_party/esp-storage
```
