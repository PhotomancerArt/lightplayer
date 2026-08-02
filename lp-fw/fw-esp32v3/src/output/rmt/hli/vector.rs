//! The level-4 RMT refill vector: hand-written Xtensa call0 assembly.
//!
//! # Provenance and clean-room statement (2026-08-02)
//!
//! Original work, written for this experiment against the state contract in
//! `lp-ws281x-hli` (whose reference model is host-tested; every field offset
//! below is derived from the struct with `offset_of!`, never transcribed).
//! Implementation references, all permissively licensed or primary
//! specifications, per `docs/adr/2026-07-29-license-provenance-discipline.md`:
//!
//! * `xtensa-lx-rt` 0.22.0 `src/exception/asm.rs` (MIT/Apache-2.0) — the
//!   vector entry contract this handler plugs into: `_Level4InterruptVector`
//!   stashes the interruptee's `a0` in `EXCSAVE4` and `call0`s
//!   `__naked_level_4_interrupt`, which the linker script exposes as a
//!   `PROVIDE` override seam. That is the sanctioned esp-hal-world hook — no
//!   vector table is stomped.
//! * esp-idf `components/bt/controller/esp32/hli_vectors.S` (Apache-2.0) —
//!   behavioral precedent only: a static save area rather than the
//!   interruptee's stack, `rfi 4` return, and the observation that a full
//!   window spill is needed **only** when calling C. This handler calls
//!   nothing, so it saves only what it clobbers.
//! * Xtensa ISA Reference Manual — call0 discipline, `EXCSAVE*`/`EPS*`/`EPC*`
//!   semantics, `rfi`, `bnone`/`bbci`, `ssr`/`srl`, `extui`, `memw`.
//! * ESP32 TRM + the `esp32` PAC 0.40.2 field docs — RMT register layout
//!   (`CHnSTATUS.mem_raddr_ex` = bits 12..=21, `CH_TX_LIM` 9-bit count,
//!   `INT_CLR` write-1-to-clear) and the interrupt-matrix (`DPORT
//!   core_0_intr_map`, CPU interrupt 24 = extern level-triggered, level 4).
//!
//! **No copyleft source was consulted. WLED and NeoPixelBus were never
//! opened** — the firewalled provenance check that preceded this experiment
//! established that WLED contains no HLI shim at any revision (its WiFi-stable
//! output is NeoPixelBus I2S/DMA, LGPL-3.0, absolutely off-limits); see the
//! ADR for the full chain.
//!
//! # Why level 4, and what it can and cannot escape
//!
//! Level 4 is the lowest priority above `EXCM_LEVEL` (3) on this core — the
//! lowest level that a `rsil 3` cannot mask. CPU interrupts at level 4 on the
//! ESP32 are {24, 25, 28, 30} (xtensa-lx-rt `XCHAL_INTLEVEL4_MASK` =
//! `0x5300_0000`); 24 and 25 are level-triggered externals (28/30 are edge —
//! esp-hal's `CPU_INTERRUPT_EDGE`), and esp-hal's allocator does not use
//! anything above 23, so **CPU interrupt 24** is free on this target. Level 5
//! (only 26 usable) would buy nothing: `esp-sync`'s critical sections are
//! `rsil 5`, so *both* 4 and 5 sit below every `critical_section::with`,
//! `esp-radio` `wifi_int_disable` and `esp-storage` flash window. What level 4
//! escapes is the `rsil ≤ 3` class — embassy/`PriorityLock` mutexes, and all
//! time spent executing level ≤ 3 handlers (including the L3 RMT dispatch
//! itself). Which class dominates the classic's masked windows is exactly
//! what the experiment measures.
//!
//! # Handler discipline (the load-bearing rules)
//!
//! * **No windowed instructions** — no `entry`, `call4/8/12`, `retw`, `rotw`.
//!   A window exception from level 4 (above `EXCM_LEVEL`) could land inside a
//!   half-handled level-3 window exception and corrupt `EPC1`/`EXCSAVE1`;
//!   avoiding window traffic entirely makes the question moot.
//! * **No memory that can fault or miss**: code and literals in `.rwtext`
//!   (IRAM, `.literal_position` keeps the `l32r` pools beside the code), data
//!   in `.bss`/statics (DRAM) and RMT MMIO. Nothing flash-mapped — this
//!   handler stays runnable inside an esp-storage cache-disabled window (it is
//!   *masked* there by the `rsil 5`, but a race on entry must not crash).
//! * **Everything touched is restored**: `a2..a13` and `SAR` via a static
//!   save area (level 4 cannot nest with itself, so one area suffices on this
//!   single-core-app firmware), `a0` from `EXCSAVE4`, exit by `rfi 4` (which
//!   restores `PS` from `EPS4` and jumps `EPC4`). `EPC1`/`EXCCAUSE`/loop
//!   registers are never written.
//! * **Ack-all storm guard**: every pending RMT cause is cleared at entry —
//!   under this feature the whole RMT interrupt line belongs to level 4, and
//!   a level-triggered cause nobody clears would re-enter forever. Channels
//!   are then serviced from the snapshot, active ones only.

use core::mem::{offset_of, size_of};

use lp_ws281x_hli::{HliBank, HliChannel};

/// The one bank the vector services. `pub(super)` so `app.rs` (thread side)
/// configures channels and reads counters; the asm reaches it via a `sym`
/// operand.
pub(super) static HLI_BANK: HliBank = HliBank::new();

/// CPU interrupt the RMT peripheral is routed to under `hli_refill`:
/// **interrupt 24** — extern, level-triggered, priority level 4 (ESP32 TRM
/// interrupt-matrix table; level membership cross-checked against
/// xtensa-lx-rt's `XCHAL_INTLEVEL4_MASK`, trigger type against esp-hal's
/// `CPU_INTERRUPT_EDGE`). esp-hal's vectored allocator tops out at CPU
/// interrupt 23, so nothing else claims it on this target.
pub(super) const HLI_CPU_INTERRUPT: u32 = 24;

/// `HliChannel` must stride with two `addi` immediates (range ±127 each) and
/// stay word-granular; checked here so a contract-crate field addition fails
/// the build instead of silently corrupting the walk.
const CH_SIZE: usize = size_of::<HliChannel>();
const CH_HALF_SIZE: usize = CH_SIZE / 2;
const _: () = assert!(CH_SIZE % 8 == 0 && CH_HALF_SIZE <= 127);
// The histogram bases must stay within l32i/s32i's 0..=1020 offset range even
// after the +32 a bucket index can add.
const _: () = assert!(offset_of!(HliChannel, lag_hist) + 4 * 8 <= 1020);

core::arch::global_asm!(
    r#"
    // Register save area (DRAM .bss): a2..a13, SAR, one stash slot for the
    // pre-refill read position. Level 4 never nests with itself, and this
    // firmware runs the app on one core, so one static area is enough — the
    // interruptee's stack is never touched (it may be mid-spill, mid-flash-op,
    // or any other half-consistent state).
    .section .bss,"aw",@nobits
    .p2align 2
_hli_l4_save:
    .space 64

    .section .rwtext,"ax",@progbits
    .literal_position
    .p2align 2
    .global __naked_level_4_interrupt
    .type __naked_level_4_interrupt,@function
__naked_level_4_interrupt:
    // Entry state (xtensa-lx-rt's _Level4InterruptVector): interruptee's a0 is
    // in EXCSAVE4; a0 currently holds the vector's call0 return address, which
    // is dead — this handler returns with rfi, not ret.
    movi    a0, _hli_l4_save
    s32i    a2, a0, 0
    s32i    a3, a0, 4
    s32i    a4, a0, 8
    s32i    a5, a0, 12
    s32i    a6, a0, 16
    s32i    a7, a0, 20
    s32i    a8, a0, 24
    s32i    a9, a0, 28
    s32i    a10, a0, 32
    s32i    a11, a0, 36
    s32i    a12, a0, 40
    s32i    a13, a0, 44
    rsr     a2, SAR
    s32i    a2, a0, 48

    movi    a2, {bank}
    // Diagnostic: count every level-4 entry, including cause-less ones.
    l32i    a3, a2, {off_entries}
    addi    a3, a3, 1
    s32i    a3, a2, {off_entries}

    // Snapshot and acknowledge EVERYTHING pending, then service from the
    // snapshot. INT_ST is raw & ena, and under this feature every enabled RMT
    // cause belongs to this handler; a cause left set on a level-triggered
    // line would re-enter endlessly.
    l32i    a3, a2, {off_int_st}
    l32i    a3, a3, 0
    l32i    a4, a2, {off_int_clr}
    s32i    a3, a4, 0
    memw
    l32i    a4, a2, {off_all_mask}
    and     a3, a3, a4
    bnez    a3, .Lhli_have_causes
    j       .Lhli_done
.Lhli_have_causes:

    // Walk the four channel entries; a3 = snapshot, a12 = remaining.
    movi    a12, {n_channels}
    addi    a2, a2, {off_channels}

.Lhli_ch_loop:
    // err: counted independently of everything else.
    l32i    a4, a2, {off_err_mask}
    bnone   a3, a4, .Lhli_no_err
    l32i    a5, a2, {off_errors}
    addi    a5, a5, 1
    s32i    a5, a2, {off_errors}
.Lhli_no_err:
    // end beats thr: a finished frame has nothing to refill.
    l32i    a4, a2, {off_end_mask}
    bnone   a3, a4, .Lhli_no_end
    l32i    a5, a2, {off_active}
    beqz    a5, .Lhli_next
    // Truncation: the cursor stopped short of the frame's bits.
    l32i    a5, a2, {off_cursor}
    l32i    a6, a2, {off_total}
    bgeu    a5, a6, .Lhli_end_counted
    l32i    a7, a2, {off_trips}
    addi    a7, a7, 1
    s32i    a7, a2, {off_trips}
.Lhli_end_counted:
    l32i    a7, a2, {off_frames}
    addi    a7, a7, 1
    s32i    a7, a2, {off_frames}
    movi    a7, 0
    s32i    a7, a2, {off_active}
    movi    a7, 1
    s32i    a7, a2, {off_complete}
    j       .Lhli_next

.Lhli_no_end:
    l32i    a4, a2, {off_thr_mask}
    bany    a3, a4, 17f
    j       .Lhli_next
17: l32i    a5, a2, {off_active}
    bnez    a5, 18f
    j       .Lhli_next
18:

    // ---- refill ----
    // pos = (mem_raddr_ex - window_start) & ram_mask.
    // CHnSTATUS.mem_raddr_ex is bits 12..=21 (esp32 PAC) — the one field
    // layout hard-coded here; everything else arrives as a precomputed
    // address or value in the channel entry.
    l32i    a4, a2, {off_status_addr}
    l32i    a4, a4, 0
    extui   a4, a4, 12, 10
    l32i    a5, a2, {off_window_start}
    sub     a4, a4, a5
    l32i    a5, a2, {off_ram_mask}
    and     a4, a4, a5

    // Entry delay against the armed boundary, before anything moves —
    // interrupt-to-service latency in 1.25 µs words, same instrument as the
    // level-3 driver's.
    l32i    a6, a2, {off_boundary}
    sub     a8, a4, a6
    and     a8, a8, a5
    l32i    a6, a2, {off_entry_max}
    bgeu    a6, a8, .Lhli_entry_nomax
    s32i    a8, a2, {off_entry_max}
.Lhli_entry_nomax:
    l32i    a9, a2, {off_half}
    movi    a5, 8
    bgeu    a8, a9, .Lhli_entry_bucketed
    l32i    a5, a2, {off_bucket_shift}
    ssr     a5
    srl     a5, a8
.Lhli_entry_bucketed:
    slli    a5, a5, 2
    add     a5, a5, a2
    l32i    a6, a5, {off_entry_hist}
    addi    a6, a6, 1
    s32i    a6, a5, {off_entry_hist}

    // Select the free half; flip the software boundary; re-arm. The classic's
    // CH_TX_LIM is a repeating count, so the armed value is always the half
    // size (mirroring v3_rmt::set_tx_threshold's clamp).
    //   reading second half -> free = first  (start 0,    guard half, boundary 0)
    //   reading first half  -> free = second (start half, guard 0,    boundary half)
    bltu    a4, a9, .Lhli_in_first
    movi    a7, 0
    mov     a11, a9
    movi    a0, 0
    j       .Lhli_selected
.Lhli_in_first:
    mov     a7, a9
    movi    a11, 0
    mov     a0, a9
.Lhli_selected:
    s32i    a0, a2, {off_boundary}
    l32i    a5, a2, {off_tx_lim_addr}
    s32i    a9, a5, 0

    // Guard word at the first slot of the half being read — unless the reader
    // still sits on it (planting a STOP there would end a healthy frame). At
    // level 4 the reader routinely IS still on the slot (entry delay 0), so
    // the miss is stashed and retried after the fill; save slot 56 holds the
    // pending guard slot, or -1.
    beq     a4, a11, .Lhli_guard_defer
    l32i    a5, a2, {off_ram_base}
    slli    a10, a11, 2
    add     a5, a5, a10
    movi    a10, 0
    s32i    a10, a5, 0
    movi    a5, -1
    movi    a10, _hli_l4_save
    s32i    a5, a10, 56
    j       .Lhli_fill
.Lhli_guard_defer:
    movi    a10, _hli_l4_save
    s32i    a11, a10, 56

.Lhli_fill:
    // Stash pos_before for the lag measurement; a4 is needed as the write
    // cursor below.
    movi    a0, _hli_l4_save
    s32i    a4, a0, 52
    // Write pointer / end pointer for the free half.
    l32i    a10, a2, {off_ram_base}
    slli    a4, a7, 2
    add     a4, a10, a4
    l32i    a9, a2, {off_half}
    slli    a9, a9, 2
    add     a7, a4, a9
    // Fill state: cursor, total, wire-order frame bytes, pulse codes.
    l32i    a6, a2, {off_cursor}
    l32i    a9, a2, {off_total}
    l32i    a8, a2, {off_frame_ptr}
    l32i    a10, a2, {off_code0}
    l32i    a11, a2, {off_code1}

    // Data: one wire-order byte per iteration, 8 RMT words, MSB first.
    // cursor/total and the half size are all multiples of 8 (contract), so
    // both exits are checked once per byte.
.Lhli_byte_loop:
    bltu    a6, a9, 19f
    j       .Lhli_data_done
19: srli    a5, a6, 3
    add     a5, a8, a5
    l8ui    a5, a5, 0
    // Per bit: default the zero code, overwrite with the one code when the
    // bit is set (bbci = branch if bit clear, skipping the overwrite).
    mov     a0, a10
    bbci    a5, 7, 1f
    mov     a0, a11
1:  s32i    a0, a4, 0
    mov     a0, a10
    bbci    a5, 6, 2f
    mov     a0, a11
2:  s32i    a0, a4, 4
    mov     a0, a10
    bbci    a5, 5, 3f
    mov     a0, a11
3:  s32i    a0, a4, 8
    mov     a0, a10
    bbci    a5, 4, 4f
    mov     a0, a11
4:  s32i    a0, a4, 12
    mov     a0, a10
    bbci    a5, 3, 5f
    mov     a0, a11
5:  s32i    a0, a4, 16
    mov     a0, a10
    bbci    a5, 2, 6f
    mov     a0, a11
6:  s32i    a0, a4, 20
    mov     a0, a10
    bbci    a5, 1, 7f
    mov     a0, a11
7:  s32i    a0, a4, 24
    mov     a0, a10
    bbci    a5, 0, 8f
    mov     a0, a11
8:  s32i    a0, a4, 28
    addi    a4, a4, 32
    addi    a6, a6, 8
    bgeu    a4, a7, .Lhli_fill_done
    j       .Lhli_byte_loop

.Lhli_data_done:
    // Tail: latch exactly once, then STOP-fill to the boundary.
    bgeu    a4, a7, .Lhli_fill_done
    l32i    a5, a2, {off_latch_written}
    bnez    a5, .Lhli_stop_fill
    l32i    a5, a2, {off_code_latch}
    s32i    a5, a4, 0
    addi    a4, a4, 4
    movi    a5, 1
    s32i    a5, a2, {off_latch_written}
.Lhli_stop_fill:
    movi    a5, 0
    bgeu    a4, a7, .Lhli_fill_done
.Lhli_stop_loop:
    s32i    a5, a4, 0
    addi    a4, a4, 4
    bltu    a4, a7, .Lhli_stop_loop

.Lhli_fill_done:
    s32i    a6, a2, {off_cursor}

    // Refill lag: words the reader advanced while this service ran.
    l32i    a4, a2, {off_status_addr}
    l32i    a4, a4, 0
    extui   a4, a4, 12, 10
    l32i    a5, a2, {off_window_start}
    sub     a4, a4, a5
    l32i    a5, a2, {off_ram_mask}
    and     a4, a4, a5
    movi    a0, _hli_l4_save
    l32i    a6, a0, 52
    sub     a4, a4, a6
    and     a4, a4, a5
    l32i    a5, a2, {off_lag_sum}
    add     a5, a5, a4
    s32i    a5, a2, {off_lag_sum}
    l32i    a5, a2, {off_lag_count}
    addi    a5, a5, 1
    s32i    a5, a2, {off_lag_count}
    l32i    a5, a2, {off_lag_max}
    bgeu    a5, a4, .Lhli_lag_nomax
    s32i    a4, a2, {off_lag_max}
.Lhli_lag_nomax:
    l32i    a9, a2, {off_half}
    movi    a5, 8
    bgeu    a4, a9, .Lhli_lag_bucketed
    l32i    a5, a2, {off_bucket_shift}
    ssr     a5
    srl     a5, a4
.Lhli_lag_bucketed:
    slli    a5, a5, 2
    add     a5, a5, a2
    l32i    a6, a5, {off_lag_hist}
    addi    a6, a6, 1
    s32i    a6, a5, {off_lag_hist}

    // Deferred guard: if the pre-fill attempt found the reader on the slot,
    // it has had a whole fill to move off it — plant now, or count the
    // refill as genuinely unguarded.
    movi    a0, _hli_l4_save
    l32i    a11, a0, 56
    movi    a5, -1
    beq     a11, a5, .Lhli_next
    l32i    a4, a2, {off_status_addr}
    l32i    a4, a4, 0
    extui   a4, a4, 12, 10
    l32i    a5, a2, {off_window_start}
    sub     a4, a4, a5
    l32i    a5, a2, {off_ram_mask}
    and     a4, a4, a5
    beq     a4, a11, .Lhli_guard_still
    l32i    a5, a2, {off_ram_base}
    slli    a11, a11, 2
    add     a5, a5, a11
    movi    a11, 0
    s32i    a11, a5, 0
    j       .Lhli_next
.Lhli_guard_still:
    l32i    a5, a2, {off_skips}
    addi    a5, a5, 1
    s32i    a5, a2, {off_skips}

.Lhli_next:
    addi    a2, a2, {ch_half_size}
    addi    a2, a2, {ch_half_size}
    addi    a12, a12, -1
    beqz    a12, .Lhli_done
    j       .Lhli_ch_loop

.Lhli_done:
    movi    a0, _hli_l4_save
    l32i    a2, a0, 48
    wsr     a2, SAR
    l32i    a2, a0, 0
    l32i    a3, a0, 4
    l32i    a4, a0, 8
    l32i    a5, a0, 12
    l32i    a6, a0, 16
    l32i    a7, a0, 20
    l32i    a8, a0, 24
    l32i    a9, a0, 28
    l32i    a10, a0, 32
    l32i    a11, a0, 36
    l32i    a12, a0, 40
    l32i    a13, a0, 44
    rsr     a0, EXCSAVE4
    rfi     4
    .size __naked_level_4_interrupt, . - __naked_level_4_interrupt

    // Restore the default section: global_asm! text is concatenated ahead of
    // the codegen unit's own assembly, and leaving `.rwtext` current would
    // strand every later function's literal pool in `.rwtext.literal`
    // (measured: `__post_init` l32r out-of-range at link).
    .section .text,"ax",@progbits
    "#,
    bank = sym HLI_BANK,
    n_channels = const lp_ws281x_hli::HLI_CHANNELS,
    off_int_st = const offset_of!(HliBank, int_st_addr),
    off_int_clr = const offset_of!(HliBank, int_clr_addr),
    off_all_mask = const offset_of!(HliBank, all_mask),
    off_entries = const offset_of!(HliBank, isr_entries),
    off_channels = const offset_of!(HliBank, channels),
    ch_half_size = const CH_HALF_SIZE,
    off_thr_mask = const offset_of!(HliChannel, thr_mask),
    off_end_mask = const offset_of!(HliChannel, end_mask),
    off_err_mask = const offset_of!(HliChannel, err_mask),
    off_status_addr = const offset_of!(HliChannel, status_addr),
    off_tx_lim_addr = const offset_of!(HliChannel, tx_lim_addr),
    off_ram_base = const offset_of!(HliChannel, ram_base),
    off_window_start = const offset_of!(HliChannel, window_start),
    off_half = const offset_of!(HliChannel, half_words),
    off_ram_mask = const offset_of!(HliChannel, ram_mask),
    off_bucket_shift = const offset_of!(HliChannel, bucket_shift),
    off_code0 = const offset_of!(HliChannel, code_zero),
    off_code1 = const offset_of!(HliChannel, code_one),
    off_code_latch = const offset_of!(HliChannel, code_latch),
    off_active = const offset_of!(HliChannel, active),
    off_frame_ptr = const offset_of!(HliChannel, frame_ptr),
    off_total = const offset_of!(HliChannel, total_bits),
    off_cursor = const offset_of!(HliChannel, bit_cursor),
    off_latch_written = const offset_of!(HliChannel, latch_written),
    off_boundary = const offset_of!(HliChannel, boundary),
    off_complete = const offset_of!(HliChannel, complete),
    off_frames = const offset_of!(HliChannel, frames),
    off_trips = const offset_of!(HliChannel, trips),
    off_skips = const offset_of!(HliChannel, skips),
    off_errors = const offset_of!(HliChannel, errors),
    off_entry_max = const offset_of!(HliChannel, entry_max),
    off_lag_sum = const offset_of!(HliChannel, lag_sum),
    off_lag_count = const offset_of!(HliChannel, lag_count),
    off_lag_max = const offset_of!(HliChannel, lag_max),
    off_entry_hist = const offset_of!(HliChannel, entry_hist),
    off_lag_hist = const offset_of!(HliChannel, lag_hist),
);

/// Route the RMT peripheral interrupt to CPU interrupt 24 (level 4) on the
/// PRO CPU and enable it. Idempotent; called once by the app-side install.
///
/// This bypasses esp-hal's vectored allocator **deliberately**: esp-hal maps
/// priorities 1..=3 onto CPU interrupts 1/19/23 and owns those; writing this
/// peripheral's own map register to an interrupt esp-hal never allocates is
/// the coexistence contract (one map register per source — mapping to 24 is
/// also what un-maps it from the level-3 path). `INTENABLE` is RMW'd via
/// `xtensa_lx` exactly as esp-hal's own `enable_cpu_interrupt_raw` does, so
/// neither side clobbers the other's bits.
pub(super) fn route_rmt_to_level4() {
    use esp_hal::peripherals::{DPORT, Interrupt};
    // SAFETY (register): the interrupt-matrix map register for the RMT source
    // holds a plain CPU-interrupt number; 24 is a valid one (TRM). Writing it
    // atomically re-routes the source; the level-3 path is never enabled for
    // RMT under this feature, so no cause is lost in the move.
    DPORT::regs()
        .core_0_intr_map(Interrupt::RMT as usize)
        .write(|w| unsafe { w.bits(HLI_CPU_INTERRUPT) });
    // SAFETY: read-modify-write of INTENABLE (xsr-based), same primitive
    // esp-hal uses; enabling a CPU interrupt whose vector is installed (the
    // global_asm above) and whose only source is the RMT line just mapped.
    unsafe {
        esp_hal::xtensa_lx::interrupt::enable_mask(1 << HLI_CPU_INTERRUPT);
    }
}
