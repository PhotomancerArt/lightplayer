# esp-alloc — LP fork

Vendored from crates.io **esp-alloc 0.10.0**, verbatim except for the diff
below. Patched in through the root `Cargo.toml`'s `[patch.crates-io]`, the
same way `third_party/esp-storage` is. The version stays `0.10.0` on purpose:
`esp-rtos` and `esp-hal` both depend on `esp-alloc` by version, and the patch
table only substitutes a source, never a version.

## The diff: the region cap is 5, not 3

Upstream hard-codes `3` in four places — `EspHeapInner::heap`, its `empty()`
initializer, `HeapStats::region_stats`, and the local that fills the latter in
`stats()`. This fork names the cap once as `pub const MAX_REGIONS: usize = 5`
and uses it in all four.

Why: the classic ESP32 firmware (`lp-fw/fw-esp32v3`) registers **four**
regions, and `add_region` panics past the cap.

| # | region | why it is separate |
|---|---|---|
| 1 | ROM PRO-CPU boot stack, `0x3FFE_0440..0x3FFE_3F20` | esp-hal reserves it; the ROM is out of it from the first Rust instruction |
| 2 | the `dram_seg` arena (`heap_allocator!`) | the only span in zero-sum competition with `.stack` |
| 3 | the SRAM1 tail above the JIT code region | `dram2_seg`, which no linker section targets |
| 4 | ROM APP-CPU boot stack, `0x3FFE_4350..` the JIT heap-span base | free once `start_app_core_isr` has bound the second core |

Naming the cap also closes an index hazard that raising the literals alone
would have opened: `stats()` writes `region_stats[id]` for every occupied slot
of `heap`, so a longer `heap` with a 3-slot `region_stats` would panic on the
fourth region's stats — not at registration, but on the first heartbeat.

Nothing else is changed but the second diff below: no allocator swap
(upstream's LLFF stays), no stats semantics, no features.

## The second diff: a heap's top is exclusive when a free finds its region

`EspHeap::dealloc` offers a pointer to each region in registration order, and
upstream's LLFF region took it when `bottom <= ptr && top >= ptr` — `top`
**inclusive**. No block of a region starts at that region's top; the first
block of a region whose span begins exactly there does. So when two regions
are address-adjacent and the lower one was registered first, freeing the
upper region's first block hands it to the lower region's hole list: a hole
past the lower region's end, which the lower region then allocates from,
over the upper region's own hole headers. The C6's E4 layout (RAM research,
2026-10-09: the main region ending at `0x4086B910` where the Rust-only tail
begins) hit it on the emulator within a minute — `Freed node (…) aliases
existing hole (…)! Bad free?` — after an 11-byte block was handed out at
`0x4086B90C`, straddling the two. Today's shipped layouts have no such pair
(the C6's radio region ends where its main region begins, but the main region
is registered first and owns that address by its `bottom`).

The fix is one comparison, `ptr < top`, the same exclusive end the TLSF
region in `src/heap/tlsf.rs` already uses (`pool_end > addr`).

## Re-syncing with upstream

Copy the new version out of the cargo registry, delete `.cargo-ok`,
`.cargo_vcs_info.json`, `Cargo.lock` and `Cargo.toml.orig` (mirroring
`third_party/esp-storage`), then re-apply the cap and the exclusive top. `grep -n '; 3\]'
src/lib.rs` finds every place upstream restates it.
