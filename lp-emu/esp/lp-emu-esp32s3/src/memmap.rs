//! The ESP32-S3's (LX7) address space.
//!
//! Provenance: every number here is read from
//! `third_party/esp-hal/ld/esp32s3/memory.x`, the `.rwdata_dummy` reservation
//! in `third_party/esp-hal/ld/esp32s3/esp32s3.x:33-37`, the vendored
//! `esp32s3_rev0_rom.elf`'s own program headers and symbol table, the
//! `esp32s3-0.35.2` PAC, or `m6/notes.md`'s static inventory of the shipped
//! image (which cites its own commands). Nothing here is inferred from a
//! datasheet PDF and nothing is guessed. Where a number is the *classic's*
//! measurement carried over, it says so — see [`PRID_CORE0`].
//!
//! # The regions, and why the splits are where they are
//!
//! ```text
//!   0x3C00_0000 +32M      DROM window   flash .rodata (cache MMU)   R
//!   0x3FC8_8000 +0x6_8000 SRAM1 (D-bus) .data/.bss/.stack/the heap  RWX
//!   0x3FF1_8C00 +0x5F3C   ROM data      mask ROM .rodata            R
//!   0x4000_0000 +0x5_77A8 mask ROM      code + the vector table     RX
//!   0x4037_0000 +0x8000   icache reserve  NAMED AND NOT MAPPED
//!   0x4037_8000 +0x6_8000 SRAM1 (I-bus) THE SAME BYTES — a RAM alias
//!   0x4200_0000 +32M      IROM window   flash .text (cache MMU)     RX
//!   0x5000_0000 +0x2000   RTC slow                                  RW
//!   0x6000_0000 +0xF_E000 MMIO          the peripheral window
//!   0x600F_E000 +0x2000   RTC fast      .rtc_fast.persistent        RWX
//! ```
//!
//! Three things to get right, each argued below and each a place a reader of
//! the classic's or the C6's map would otherwise assume wrongly.
//!
//! # (a) SRAM1 is ONE region with TWO views, and the alias is used
//!
//! The physical block is `0x3FC8_8000..0x3FCF_0000` on the data bus and
//! `0x4037_8000..0x403E_0000` on the instruction bus — `memory.x:13-14` draws
//! exactly that picture — and `0x4037_8000 - 0x3FC8_8000 = 0x6F_0000`.
//!
//! This machine registers **one** region at the D-bus base
//! ([`SRAM1_DBUS_BASE`], where `.data`, `.bss`, the stack and the heap live)
//! and adds the I-bus view with `SocBus::add_ram_alias` (M6 P02). It is not a
//! second region: two regions would be two independent stores, and a write
//! through one would be invisible through the other — the mistake DD24/DD36
//! refused on the classic, in the one place on this chip where it would
//! really bite.
//!
//! **And it really would bite.** Statically no executable section is placed
//! through the D-bus view (`m6/notes.md` §2.6: `.vectors`, `.rwtext` and
//! `.text` are all `AX`, on the I-bus or in flash). Dynamically the product
//! path does exactly that on every shader the firmware compiles:
//! `fw_esp32s3::boot_firmware::HEAP` is at `0x3FC9_12B1 +0x3C000`, entirely
//! inside the dual-mapped window, and every JIT entry address and
//! intra-module `callx8` target is `write + 0x6F_0000`
//! (`lp-shader/lpvm-native/src/exec_addr.rs:17-29,37,40,61-67`). A machine
//! that mapped only the ELF's sections would boot this firmware perfectly and
//! fault on the first shader.
//!
//! ⚠️ **Why the D-bus region carries the executable flag.** `SocBus`
//! translates an alias address to its canonical (target) one *first*, before
//! the region lookup, the access rule, the watchpoints, the cost model and
//! the guest-code spans (P02's `canonical()`, ruling DD81). So a fetch
//! through `0x4037_8400` arrives at the region lookup as `0x3FC8_8400`, and
//! it is the **D-bus region's** `executable` flag that decides whether the
//! fetch is allowed. That is why [`crate::bus_setup::EXECUTABLE`] names
//! `sram1-dbus`, and it is not an accident to be tidied away later.
//!
//! The same rule is why M7's translator and P07's frame test inherit a
//! working answer for free: the ninth of the classic's render loop that is
//! guest-JIT code with no symbols (M7 P1b's 11.9 % finding) is, on this chip,
//! written through the D-bus view and fetched through the I-bus one, and
//! `canonical()` sits before the guest-code spans.
//!
//! ⚠️ **The apparent `iram_seg`/`dram_seg` overlap in `memory.x` is resolved
//! by the linker script, not by luck.** `.rwdata_dummy` is a `NOBITS WA`
//! section at `0x3FC8_8000` of size `0x3320` — exactly `SIZEOF(.vectors) +
//! SIZEOF(.rwtext)`, `0x400 + 0x2F20`
//! (`third_party/esp-hal/ld/esp32s3/esp32s3.x:33-37`) — the D-bus shadow of
//! the I-bus code, with `.data` starting immediately above it at
//! `0x3FC8_B320`. A reader who sees the overlap and not the reservation will
//! think this map is wrong. See [`RWDATA_DUMMY_LEN`].
//!
//! # (b) The icache reserve is named and NOT mapped
//!
//! `0x4037_0000..0x4037_8000` is `RESERVE_ICACHE` (`memory.x:1-7`), which is
//! why `vectors_seg` starts at `0x4037_0000 + RESERVE_ICACHE`. With the cache
//! on, that memory **is** the cache array.
//!
//! The classic's answer to the same situation was one executable region from
//! the bottom (DD24 R1), because the ESP-IDF second-stage bootloader really
//! executes inside SRAM0's `reserved_cache_seg` — a fact established from the
//! ROM-up path's own behaviour. **This machine has no ROM-up path yet**
//! (P06's), so there is no S3 evidence of anything executing there, and this
//! phase invents none: the span is named ([`ICACHE_RESERVE_BASE`]) and left
//! unmapped, so a strict stop inside it says which window it was instead of
//! the far less useful "unmapped". P06 establishes the S3's equivalent from
//! the ROM-up chain and, if the bootloader does run there, maps it then — a
//! decision, not a discovery.
//!
//! # (c) RTC fast is real on this chip, and it is inside the MMIO window
//!
//! `.rtc_fast.persistent` at `0x600F_E000` carries `lp_recovery`'s ledger and
//! its crash snapshots: 29 access sites across 20 symbols, including offsets
//! `+0x1C00` and `+0x1C08` (`m6/notes.md` §2.4). `memory.x:44` declares it
//! `RWX`, 8 KiB; its `.rtc_fast.text` is size 0 in this image (§2.6), so
//! nothing executes there today and the region is executable anyway because
//! the linker script says the memory is.
//!
//! Two consequences worth stating:
//!
//! - **The classic's RTC-fast alias policy (DD36) does not arise here.** The
//!   classic has the same 8 KiB behind two addresses (`0x3FF8_0000` and
//!   `0x400C_0000`) and maps one; the S3's RTC fast is reached at
//!   `0x600F_E000` **only**. One address, one store, no ruling needed.
//! - **The declared MMIO window stops where RTC fast begins.** The nominal
//!   peripheral aperture is `0x6000_0000 +1M`, and RTC fast is its top 8 KiB.
//!   A RAM region wins the bus's decode over an MMIO window either way, but
//!   declaring the window *through* RTC fast would make a strict stop at
//!   `0x600F_Exxx` report "an unmodelled block" when it is really memory. So
//!   [`MMIO_LEN`] ends at [`RTC_FAST_BASE`] and the map says why.
//!
//! # What this chip does NOT have that the classic does
//!
//! - **No word-only region.** This map declares no `AccessRule` beyond the
//!   default: the classic's SRAM0 rule is a *measurement* on classic silicon
//!   (`lp-xt-emu/src/board.rs:156-172` — a byte store at `0x4008_8000`
//!   faulted, 16,384 aligned word stores read back), and no such measurement
//!   exists for the S3. Inventing one would be the thing this plan exists to
//!   stop.
//! - **No second peripheral bus.** The classic mirrors its DPORT window onto
//!   the AHB bus (DD38); the S3's blocks live at one address each, all inside
//!   [`MMIO_BASE`].

// ---------------------------------------------------------------------------
// The flash windows
// ---------------------------------------------------------------------------

/// The DROM window: the app's `.rodata`, mapped through the cache MMU.
///
/// `memory.x:40` links `drom_seg` at `0x3C00_0020 + (32M - 0x20)`; the `0x20`
/// is the image-header convenience that satisfies the MMU's
/// `paddr % 64K == vaddr % 64K` (`memory.x:32-38` says so in as many words),
/// not a hardware boundary — so the region starts at the window base and the
/// first 32 bytes are addressable rather than a hole. The shipped image's
/// first `PT_LOAD` is at `0x3C00_0020` (`m6/notes.md` §2.6).
pub const DROM_BASE: u32 = 0x3C00_0000;

/// 32 MiB (`memory.x:40`).
///
/// ⚠️ This is the **window** the linker script declares, not a claim about how
/// much flash the part carries or how much of it the cache MMU can serve at
/// once. P06 owns the MMU and the chip's real size.
pub const DROM_LEN: u32 = 0x0200_0000;

/// The IROM window: the app's `.text`. `memory.x:39`, `0x4200_0020 +
/// (32M - 0x20)`; same `0x20` reasoning as [`DROM_BASE`]. The shipped image's
/// `.text` is at `0x4205_0020` (`m6/notes.md` §2.6).
pub const IROM_BASE: u32 = 0x4200_0000;

/// 32 MiB (`memory.x:39`). [`DROM_LEN`]'s caveat applies.
pub const IROM_LEN: u32 = 0x0200_0000;

// ---------------------------------------------------------------------------
// SRAM1 — the one region with two views
// ---------------------------------------------------------------------------

/// SRAM1 seen from the **data** bus: `dram_seg`'s origin (`memory.x:30`), and
/// the base of the D/IRAM block `memory.x:13-14` draws.
///
/// `.data` (`0x3FC8_B320`), `.bss` (`0x3FC9_0F08..0x3FCD_2560`), `.noinit`,
/// `.stack` (`0x3FCD_2560..0x3FCD_B700`) and the heap all live above it, and
/// so does the mask ROM's own `.bss`/`.stack_pro`/`.stack_app` block
/// (`0x3FCD_7000 +0x1_8770`, from the ROM ELF's first `PT_LOAD`).
pub const SRAM1_DBUS_BASE: u32 = 0x3FC8_8000;

/// `0x6_8000` — `0x3FC8_8000..0x3FCF_0000`, the block `memory.x:13-14` draws
/// and the exact length of the I-bus view above it.
///
/// `dram_seg` itself is only `0x5_3700` ([`DRAM_SEG_LEN`]) and `dram2_seg`
/// `0x1_2010` ([`DRAM2_SEG_LEN`]) — the linker's two halves of the same
/// memory, split where the second-stage bootloader stops needing the top.
/// Both are inside this one region, which is registered once because it is
/// one memory.
pub const SRAM1_LEN: u32 = 0x0006_8000;

/// SRAM1 seen from the **instruction** bus: `vectors_seg`'s origin,
/// `0x4037_0000 + RESERVE_ICACHE` (`memory.x:25`), which with the 32 KB
/// instruction cache this image is built for is `0x4037_8000`.
///
/// **Not a region.** [`crate::bus_setup`] registers it as a RAM alias onto
/// [`SRAM1_DBUS_BASE`]; see the module docs.
pub const SRAM1_IBUS_BASE: u32 = 0x4037_8000;

/// `0x6F_0000` — the distance between the two views, confirmed three ways:
/// the two bases above, `memory.x:13-14`'s own diagram, and
/// `lp-shader/lpvm-native/src/exec_addr.rs:37`, which is the constant the
/// product's JIT adds to every entry address it publishes.
pub const SRAM1_IBUS_OFFSET: u32 = SRAM1_IBUS_BASE - SRAM1_DBUS_BASE;

/// `RESERVE_ICACHE` (`memory.x:1-7`): `0x8000` for the 32 KB instruction
/// cache, `0x4000` for the 16 KB one. This image is built for 32 KB — the
/// evidence is that its `.vectors` is at `0x4037_8000` and not `0x4037_4000`
/// (`m6/notes.md` §2.6).
pub const RESERVE_ICACHE: u32 = 0x8000;

/// The base of the icache reserve, `0x4037_0000` (`memory.x:13`).
///
/// **DELIBERATELY UNMAPPED**, and named so a strict stop says which window it
/// was — see the module docs (b).
pub const ICACHE_RESERVE_BASE: u32 = SRAM1_IBUS_BASE - RESERVE_ICACHE;

/// `vectors_seg` (`memory.x:25`): the app's 1 KiB vector table, the first
/// kilobyte of the I-bus view.
pub const VECTORS_BASE: u32 = SRAM1_IBUS_BASE;

/// `VECTORS_SIZE` (`memory.x:9`).
pub const VECTORS_LEN: u32 = 0x400;

/// `iram_seg` (`memory.x:26`): the app's `.rwtext`, immediately above the
/// vectors.
pub const IRAM_SEG_BASE: u32 = VECTORS_BASE + VECTORS_LEN;

/// `328k - VECTORS_SIZE - RESERVE_ICACHE` (`memory.x:26`), which is
/// `0x4_9C00` and ends at `0x403C_2000` — **short of** the I-bus view's end at
/// `0x403E_0000`. That is the linker script being conservative about what
/// static placement may use ("Startup code uses the IRAM from 0x403B9000 to
/// 0x403E0000", `memory.x:16-17`), not a statement about what memory exists.
/// The region this machine maps is the whole block; this constant is here so
/// a test can say where the linker would stop.
pub const IRAM_SEG_LEN: u32 = 328 * 1024 - VECTORS_LEN - RESERVE_ICACHE;

/// `.rwdata_dummy`'s size: `SIZEOF(.vectors) + SIZEOF(.rwtext)` =
/// `0x400 + 0x2F20` (`third_party/esp-hal/ld/esp32s3/esp32s3.x:33-37` and
/// `m6/notes.md` §2.6).
///
/// The `NOBITS WA` reservation at [`SRAM1_DBUS_BASE`] that keeps `.data` off
/// the D-bus shadow of the I-bus code. See the module docs — this is why the
/// apparent `memory.x` overlap is not one.
pub const RWDATA_DUMMY_LEN: u32 = 0x3320;

/// Where `.data` starts, immediately above [`RWDATA_DUMMY_LEN`]
/// (`m6/notes.md` §2.6). Reporting and test use only.
pub const DATA_START: u32 = SRAM1_DBUS_BASE + RWDATA_DUMMY_LEN;

/// `dram_seg` (`memory.x:30`): `ORIGIN = 0x3FC8_8000`. Inside [`SRAM1_LEN`];
/// see it for why both halves are one region.
pub const DRAM_SEG_BASE: u32 = SRAM1_DBUS_BASE;

/// `len = ORIGIN(dram2_seg) - 0x3FC8_8000` = `0x5_3700` (`memory.x:30`).
pub const DRAM_SEG_LEN: u32 = DRAM2_SEG_BASE - DRAM_SEG_BASE;

/// `dram2_seg` (`memory.x:29`): "memory available after the 2nd stage
/// bootloader is finished", `ORIGIN = 0x3FCD_B700`.
pub const DRAM2_SEG_BASE: u32 = 0x3FCD_B700;

/// `0x3FCE_D710 - 0x3FCD_B700` = `0x1_2010` (`memory.x:29`). Its end is also
/// the mask ROM's `__stack_app` ([`ROM_APP_STACK_TOP`]) — two sources, one
/// number, the same not-a-coincidence the classic's map records.
pub const DRAM2_SEG_LEN: u32 = 0x3FCE_D710 - DRAM2_SEG_BASE;

// ---------------------------------------------------------------------------
// The mask ROM
// ---------------------------------------------------------------------------

/// Mask ROM code, including the vector table at its base.
///
/// The vendored `esp32s3_rev0_rom.elf` puts `.WindowVectors.text` at
/// `0x4000_0000` and `_ResetVector` at `0x4000_0400`, which is also the ELF's
/// `e_entry` — read off its own program headers and symbol table.
pub const ROM_MASK_BASE: u32 = 0x4000_0000;

/// Through `0x4005_8190`: the ROM ELF's last executable `PT_LOAD` ends at
/// `0x4000_0400 + 0x573A8 = 0x4005_77A8`, and **the ROM's data image
/// follows it in the same silicon** — `0x4005_77A8` is the load address its
/// first `.data` segment carries as a `paddr`, and the reset handler's
/// unpack loop (`_ResetHandler` `4000050b <unpackloop>`, the table at
/// `_data_start` `0x4005_7354`..`_data_end` `0x4005_75C4`) copies every
/// `.data_*` and `.data.interface.*` section from a source in
/// `0x4005_77A8..0x4005_8190` to its RAM address. The highest source byte
/// any of the 39 entries names is `0x4005_8190` (`0x4005_818C + 4`, the
/// `.data_ets_delay` entry) — which is also the ROM ELF's own **`_at_text`**
/// symbol, the end of its image — and [`crate::rom::seed_data_image`] fills
/// that span; `tests/rom_vendoring.rs` re-derives this end from the table
/// and the symbol.
///
/// ⚠️ P03 stopped the region at `0x4005_77A8` because a direct load never
/// runs the unpack loop; the first ROM-up boot (P06) stopped at
/// `unpcopy` `0x4000_0522` reading `0x4005_77A8`, cycle 162.
pub const ROM_MASK_LEN: u32 = 0x0005_8190;

/// Mask ROM read-only data. `.rodata` has **vaddr `0x3FF1_8C00`, paddr
/// `0x4005_8C00`** — they differ, so a loader must place by `p_vaddr`.
pub const ROM_DATA_BASE: u32 = 0x3FF1_8C00;

/// `0x7400` — through `0x3FF2_0000`, and **not** the `.rodata` `PT_LOAD`'s
/// `0x5F3C`.
///
/// ⚠️ **Found the hard way, which is what a stop-and-report loader is for.**
/// The window first read as `0x5F3C`, the length of the `.rodata` `PT_LOAD`
/// (`0x3FF1_8C00..0x3FF1_EB3C`). The first strict build then refused with
/// `RomError::Unmapped { vaddr: 0x3ff1ee50, memsz: 0x11b0 }`: the ROM carries
/// a **second** read-only chunk, `.rodata.interface` — a `PROGBITS` section
/// flagged `W` and not `A`, so no program header covers it and only
/// [`crate::rom::seed_data`] places it (`readelf -SW esp32s3_rev0_rom.elf`,
/// section 94: `3ff1ee50 … 0011b0 W`). It ends exactly at `0x3FF2_0000`.
///
/// So the region runs to `0x3FF2_0000` and the `0x314` bytes between the two
/// chunks are inside it — mapped and zero, which is what a ROM's own address
/// space looks like. The alternative, two regions with a hole, would make a
/// stray read of that gap a fault this map could not explain.
pub const ROM_DATA_LEN: u32 = 0x0000_7400;

/// `_stack_sentry` in the ROM ELF, `0x3FCE_9710` — the bottom of the mask
/// ROM's PRO-core stack, so that stack is 8 KiB.
pub const ROM_PRO_STACK_BASE: u32 = 0x3FCE_9710;

/// The top of the mask ROM's **PRO**-core stack — `__stack` in the ROM ELF's
/// symbol table, `0x3FCE_B710`.
///
/// This is the value a direct load seeds as the outermost frame's stack
/// pointer ([`crate::machine::BootFrame`]) **until P06 derives the ESP-IDF
/// bootloader's own SP from the ROM-up chain**. See `BootFrame`'s docs for
/// what that seam claims and what it does not.
pub const ROM_PRO_STACK_TOP: u32 = 0x3FCE_B710;

/// The top of the mask ROM's **APP**-core stack: `__stack_app` in the ROM
/// ELF, `0x3FCE_D710`, which is also `dram2_seg`'s end ([`DRAM2_SEG_LEN`]).
///
/// Nothing in this phase uses it — slot 1 is held and there is no release
/// (see [`crate::machine`]) — and it is named because a P04 that wires
/// `SYSTEM.core_1_control_0` to a real start will want it, and because a
/// number with one home cannot drift.
pub const ROM_APP_STACK_TOP: u32 = 0x3FCE_D710;

// ---------------------------------------------------------------------------
// RTC memory
// ---------------------------------------------------------------------------

/// `rtc_fast_seg` (`memory.x:44`), 8 KiB, `RWX`, PRO core only.
/// `.rtc_fast.persistent` — `lp_recovery`'s ledger — lives here.
pub const RTC_FAST_BASE: u32 = 0x600F_E000;

/// 8 KiB (`memory.x:44`).
pub const RTC_FAST_LEN: u32 = 0x0000_2000;

/// `rtc_slow_seg` (`memory.x:47`), 8 KiB, data only.
pub const RTC_SLOW_BASE: u32 = 0x5000_0000;

/// 8 KiB (`memory.x:47`).
pub const RTC_SLOW_LEN: u32 = 0x0000_2000;

// ---------------------------------------------------------------------------
// MMIO
// ---------------------------------------------------------------------------

/// The peripheral window. Every base in [`periph`] falls inside it.
pub const MMIO_BASE: u32 = 0x6000_0000;

/// `0xF_E000`, ending exactly at [`RTC_FAST_BASE`].
///
/// The nominal aperture is 1 MiB; RTC fast memory is its top 8 KiB. The
/// window is declared short of it so a strict stop inside RTC fast cannot
/// report "an unmodelled block" for what is really memory — see the module
/// docs (c).
pub const MMIO_LEN: u32 = RTC_FAST_BASE - MMIO_BASE;

/// Peripheral base addresses, from `esp32s3-0.35.2/src/lib.rs` (every
/// peripheral is `Periph<RegisterBlock, BASE>`; the line number is on each),
/// filtered by the MMIO census in `m6/notes.md` §2.4 — which is what says
/// **which** of the PAC's 57 blocks the shipped image actually reaches.
///
/// **Nothing in P03 registers one.** This phase's bring-up ends at its first
/// strict stop inside [`MMIO_BASE`]'s window, and that stop is the evidence
/// it worked; P04 registers the blocks. The bases are here so that P04 adds
/// models rather than addresses, and so `--map` can say what is coming.
pub mod periph {
    /// `:983`. SPI1 — the flash controller `esp_storage` drives.
    pub const SPI1: u32 = 0x6000_2000;
    /// `:974`. SPI0 — the cache's own flash port.
    pub const SPI0: u32 = 0x6000_3000;
    /// `:704`.
    pub const GPIO: u32 = 0x6000_4000;
    /// `:722`. One of the four RF-adjacent blocks `esp_hal::init` touches on
    /// its inlined path — one register each, and all four arrive in the first
    /// few thousand cycles of a strict bring-up.
    pub const FE2: u32 = 0x6000_5000;
    /// `:731`. See [`FE2`].
    pub const FE: u32 = 0x6000_6000;
    /// `:686`. MAC and chip revision.
    pub const EFUSE: u32 = 0x6000_7000;
    /// `:911`. Reset cause, the stall keys, the RWDT and the super-watchdog.
    pub const RTC_CNTL: u32 = 0x6000_8000;
    /// `:803`.
    pub const IO_MUX: u32 = 0x6000_9000;
    /// The analog I2C master behind `clocks::request_pll_clk` — the classic's
    /// fifth strict stop, one chip over. ⚠️ **The PAC names no block here**;
    /// the census's address is `0x6000_E040`, the command word
    /// (`m6/notes.md` §2.4), and this is that window's start. P04 cites the
    /// ROM for it, never a datasheet, exactly as the classic does.
    pub const I2C_ANA_MST: u32 = 0x6000_E000;
    /// `:884`.
    pub const RMT: u32 = 0x6001_6000;
    /// RMT's own pulse memory. ⚠️ `+0x800` on this part, from the firmware
    /// and a bench probe — **P07 verifies it**; `m6/notes.md` §3.5 records a
    /// `+0x400` mis-conversion that looked plausible.
    pub const RMT_RAM: u32 = RMT + 0x800;
    /// `:839`. See [`FE2`].
    pub const NRX: u32 = 0x6001_CC00;
    /// `:650`. See [`FE2`].
    pub const BB: u32 = 0x6001_D000;
    /// `:1028`.
    pub const TIMG0: u32 = 0x6001_F000;
    /// `:1037`. Shares `timg0::RegisterBlock` with [`TIMG0`].
    pub const TIMG1: u32 = 0x6002_0000;
    /// `:1019`. `esp_rtos::now` reads it; the S3's clock.
    pub const SYSTIMER: u32 = 0x6002_3000;
    /// `:632`.
    pub const APB_CTRL: u32 = 0x6002_6000;
    /// `:1100`. The console and the only link: `esp-println`'s `jtag-serial`
    /// over USB-Serial-JTAG (P05).
    pub const USB_DEVICE: u32 = 0x6003_8000;
    /// `:965`. ROM-up only, for the IDF bootloader's image hash — the shipped
    /// image never touches it (`m6/notes.md` §2.4).
    pub const SHA: u32 = 0x6003_B000;
    /// `:1010`. The peripheral clock and reset gates, and — the part this
    /// phase cares about — `core_1_control_0` at `+0x00`.
    pub const SYSTEM: u32 = 0x600C_0000;
    /// `:956`. ⚠️ **The first strict stop's block, and the census does not
    /// name it.**
    ///
    /// `m6/notes.md` §2.4 lists `sensitive` under "not touched", and that is
    /// correct about the *application*: the census swept the shipped image's
    /// own `l32r` literals, and no application symbol reaches this block. The
    /// first access on a direct load comes from the **mask ROM** —
    /// `Cache_Occupy_ICache_MEMORY+0xc` at `0x4004_F670` reads
    /// `+0x04` (`cache_dataarray_connect_1`, "Cache data array configuration
    /// register 1") thirty-six cycles in, on `esp32_init`'s
    /// `rom_config_instruction_cache_mode` path — which a literal sweep of the
    /// application could not see. P03's bring-up run is the evidence and P04
    /// models it first.
    pub const SENSITIVE: u32 = 0x600C_1000;
    /// `:785`. ⚠️ `INTERRUPT_CORE0` and `INTERRUPT_CORE1` are **one 4 KiB
    /// window**, core 0 at `+0x000` and core 1 at `+0x800`. The PAC gives both
    /// types the same base and that is correct, not an SVD leak.
    pub const INTERRUPT_CORE0: u32 = 0x600C_2000;
    /// [`INTERRUPT_CORE0`] `+0x800`.
    pub const INTERRUPT_CORE1: u32 = INTERRUPT_CORE0 + 0x800;
    /// `:695`. The cache and the flash MMU. ⚠️ Its cache-enable polarity is
    /// **inverted** relative to the C6's: `icache_ctrl.icache_enable` bit 0 is
    /// "0 disable, 1 enable" where the C6's `l1_icache_shut_ibus0` is "0
    /// enable, 1 disable". A watch copied from the C6 arms backwards (P06).
    pub const EXTMEM: u32 = 0x600C_4000;
    /// `:1055`. ⚠️ **Not touched by the shipped image at all**
    /// (`m6/notes.md` §2.4): the S3's console is USB-Serial-JTAG. It is here
    /// for the mask ROM's own console on a ROM-up boot and for nothing else —
    /// which makes the S3 the first machine in this plan with no UART on the
    /// application path. Its base is also [`super::MMIO_BASE`].
    pub const UART0: u32 = 0x6000_0000;
    /// `:1064`. Nothing opens it; the mask ROM's `uartAttach` (`0x4004_8860`)
    /// writes its `int_clr` (`40048897: l32r a8, 60010010`) on every
    /// ROM-up boot, so it gets the same view as UART0 (P06).
    pub const UART1: u32 = 0x6001_0000;
    /// The flash MMU page table — **not in the PAC**, and not in
    /// [`EXTMEM`]'s block: `Cache_MMU_Init` (`0x4004_f6f4`) writes 512 words
    /// starting here (`4004f6f7: l32r a9, 600c5000`). See
    /// [`crate::cache`] (P06).
    pub const FLASH_MMU: u32 = 0x600C_5000;
    /// `:659`. Not in the census: the mask ROM's `boot_prepare`
    /// (`40043883: l32r a2, 600ce05c` — `core_0_debug_mode`) reads it on
    /// every ROM-up boot, and `assist_debug_record_enable` (`0x4004_36f4`)
    /// writes `+0x48`/`+0x4c`. The S3's first ROM-up boot stopped here at
    /// cycle 21,846 (P06); an accept block.
    pub const ASSIST_DEBUG: u32 = 0x600C_E000;
    /// `:641`. Not in the census: the IDF bootloader's RNG early entropy
    /// source reads `+0x70` — the S3's second ROM-up strict stop, cycle
    /// 12,482,844 (P06). An accept block.
    pub const APB_SARADC: u32 = 0x6004_0000;
    /// `:947`. The same entropy source's other half. An accept block (P06).
    pub const SENS: u32 = 0x6000_8800;
    /// `:893`. The PAC's `RNG` block: one register, `data` at `+0x110` —
    /// `0x6003_507C`, the word `bootloader_fill_random` reads for the
    /// image-hash salt. A seeded generator (P06, ruling R4).
    pub const RNG: u32 = 0x6003_4F6C;
}

/// `SYSTEM.core_1_control_0`'s address — the register that holds core 1 on
/// this part (`m6/notes.md` §3.5: `control_core_1_runstall` bit 0,
/// `control_core_1_clkgate_en` bit 1, `control_core_1_reseting` bit 2).
///
/// P04 models it; [`crate::machine::Machine::core_stalled`] already asks the
/// handle P04 will fill, so a guest that tried to start core 1 meets a
/// modelled hold rather than an unmapped stop — and a future S3 firmware with
/// a second core is a P04 change, not a machine rewrite.
pub const SYSTEM_CORE_1_CONTROL_0: u32 = periph::SYSTEM;

// ---------------------------------------------------------------------------
// The cores
// ---------------------------------------------------------------------------

/// `PRID` on core 0.
///
/// ⚠️ **The only field anything in this repository reads is bit 13.**
/// esp-hal's `raw_core()` is `get_processor_id() & 0x2000`
/// (`third_party/esp-hal/src/system.rs:297-315`), and the shipped image's 123
/// `rsr.prid` sites all reach it through that. Bit 13 is clear here.
///
/// The **word** is the classic's measurement carried over
/// (`lp-emu-esp32v3/src/machine.rs`'s `PRID_PRO`), and it is not
/// independently evidenced for this part: a scan of the vendored
/// `esp32s3_rev0_rom.elf` for the literal `0x0000_CDCD` finds it only inside
/// `__udivmoddi4`, where it is division arithmetic and not a core id.
/// [`PRID_CORE1`] *is* evidenced. P09's silicon capture is what would pin
/// this one; until then the bit the firmware reads is right and the rest is
/// inherited, which is said rather than implied.
pub const PRID_CORE0: u32 = 0x0000_CDCD;

/// `PRID` on core 1, with bit 13 set.
///
/// **Cited to this chip's own ROM**: `0x0000_ABAB` is a literal at
/// `0x4000_0404` in the vendored `esp32s3_rev0_rom.elf` — four bytes past
/// `_ResetVector` (a 3-byte jump), i.e. in the reset vector's own literal
/// pool, which is exactly where the classic ROM's `_ResetHandler` loads it to
/// compare against `rsr.prid`. Slot 1 is held in this phase and never reads
/// it; the number is pinned anyway, because a held core still answers
/// `rsr.prid` in a snapshot and in `--probe`.
pub const PRID_CORE1: u32 = 0x0000_ABAB;

/// CPU clock: 240 MHz. `CpuClock::max()` on this chip
/// (`third_party/esp-hal/src/clock/mod.rs:95-107` — the `else` arm — and
/// `lp-fw/fw-esp32s3/src/board/esp32s3/init.rs:53`, which sets it). Guest
/// microseconds are `cycles / 240`.
pub const CPU_HZ: u64 = 240_000_000;

/// Cycles per emulated microsecond.
pub const CYCLES_PER_US: u64 = CPU_HZ / 1_000_000;

// ---------------------------------------------------------------------------
// The spans
// ---------------------------------------------------------------------------

/// One named span of the map, for `--map` and for the tests that assert the
/// map does not drift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub name: &'static str,
    pub base: u32,
    pub len: u32,
}

impl Span {
    pub const fn end(&self) -> u32 {
        self.base + self.len
    }

    pub const fn contains(&self, address: u32) -> bool {
        address >= self.base && address < self.base + self.len
    }
}

/// The RAM-backed regions, in address order — what [`crate::bus_setup::build`]
/// registers, one store each.
///
/// ⚠️ **The SRAM1 I-bus view is not in this list.** It is not a region; it is
/// a [`RAM_ALIASES`] entry onto `sram1-dbus`. A machine that listed it here
/// would be back to two stores for one memory.
pub const RAM_SPANS: &[Span] = &[
    Span {
        name: "drom-window",
        base: DROM_BASE,
        len: DROM_LEN,
    },
    Span {
        name: "sram1-dbus",
        base: SRAM1_DBUS_BASE,
        len: SRAM1_LEN,
    },
    Span {
        name: "rom-data",
        base: ROM_DATA_BASE,
        len: ROM_DATA_LEN,
    },
    Span {
        name: "rom-mask",
        base: ROM_MASK_BASE,
        len: ROM_MASK_LEN,
    },
    Span {
        name: "irom-window",
        base: IROM_BASE,
        len: IROM_LEN,
    },
    Span {
        name: "rtc-slow",
        base: RTC_SLOW_BASE,
        len: RTC_SLOW_LEN,
    },
    Span {
        name: "rtc-fast",
        base: RTC_FAST_BASE,
        len: RTC_FAST_LEN,
    },
];

/// A second door onto bytes a region already owns: `(alias span, canonical
/// base)`.
///
/// One entry, and it is the reason M6 P02 exists — see the module docs (a).
pub const RAM_ALIASES: &[(Span, u32)] = &[(
    Span {
        name: "sram1-ibus",
        base: SRAM1_IBUS_BASE,
        len: SRAM1_LEN,
    },
    SRAM1_DBUS_BASE,
)];

/// The declared MMIO windows. An access inside one that no peripheral claims
/// is still unmapped, but the log can say "an unmodelled block" rather than
/// "unmapped".
///
/// One window on this chip: the S3 has no AHB mirror (DD38 is the classic's).
pub const MMIO_WINDOWS: &[Span] = &[Span {
    name: "mmio",
    base: MMIO_BASE,
    len: MMIO_LEN,
}];

/// Windows this machine names and deliberately does **not** map. A strict stop
/// inside one says which window it was, instead of "unmapped".
///
/// Today that is the icache reserve alone — see the module docs (b).
pub const UNMAPPED_BY_DESIGN: &[Span] = &[Span {
    name: "icache-reserve",
    base: ICACHE_RESERVE_BASE,
    len: RESERVE_ICACHE,
}];
