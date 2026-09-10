/* Memory map for the `mach` fixtures — one RAM-only ESP32-S3 SRAM1 region.
 *
 * The host test builds the bus with `Memory::add_sram1(0x3FC8_8000, 0x28000)`:
 * 160 KiB of SRAM1, writable at its D-bus addresses and fetchable at the I-bus
 * alias `+0x6F_0000` (lp-emu/lp-xt-emu/src/memory.rs). Every segment below is
 * one view or the other of that single backing store:
 *
 *   segment      I-bus                  D-bus (backing)
 *   vectors_seg  0x40378000..0x40378400 0x3FC88000..0x3FC88400
 *   ROTEXT       0x40378400..0x40390000 0x3FC88400..0x3FCA0000
 *   RODATA       (0x40390000)           0x3FCA0000..0x3FCA4000
 *   RWTEXT       0x40394000..0x40398000 0x3FCA4000..0x3FCA8000
 *   RWDATA       (0x40398000)           0x3FCA8000..0x3FCAC000
 *   stack        —                      0x3FCAC000..0x3FCAF000 (grows down)
 *   phantom      —                      0x3FCAF000..0x3FCAF080
 *
 * Two constraints pin these numbers rather than taste:
 *
 *  - `vectors_seg` must be 1 KiB aligned: `Reset` does `wsr.vecbase
 *    _init_start`, and VECBASE's low 10 bits are not writable. 0x40378000 is
 *    both 1 KiB aligned and the base of the modeled region.
 *  - Fixture (a) walks the stack with `lpc_shared::backtrace`, whose ESP32-S3
 *    window set accepts text in `0x4037_0000..0x403E_0000` and stacks in
 *    `0x3FC8_8000..0x3FD0_0000`. Every address above is inside those, so a
 *    frame that the walker rejects is a real finding rather than a layout
 *    accident. (The walker reports ZERO frames when miscalibrated, which reads
 *    as a forensic result — see that module's comment.)
 *
 * `.data` is deliberately EMPTY in every fixture: xtensa-lx-rt's `xtensa.in.x`
 * links `.data` with `AT > RODATA`, so an initialized static would get an LMA
 * distinct from its VMA, and `lp-xt-elf`'s loader writes PT_LOAD segments to
 * `p_vaddr` (not `p_paddr`) — `Reset`'s `.data` copy would then read zeros over
 * the real bytes. The host test asserts `_data_start == _data_end` rather than
 * trusting this comment. Use `static mut X: u32 = 0;` (`.bss`) or a `const`.
 */

MEMORY
{
  vectors_seg ( RX  ) : ORIGIN = 0x40378000, LENGTH = 0x400
  ROTEXT      ( RX  ) : ORIGIN = 0x40378400, LENGTH = 0x17C00
  RODATA      ( R   ) : ORIGIN = 0x3FCA0000, LENGTH = 0x4000
  RWTEXT      ( RWX ) : ORIGIN = 0x40394000, LENGTH = 0x4000
  RWDATA      ( RW  ) : ORIGIN = 0x3FCA8000, LENGTH = 0x4000
}

/* Where `Reset` puts SP. 16-byte aligned (the windowed ABI's stack alignment,
 * which `lpc_shared::backtrace::is_valid_xt_stack` enforces), with 12 KiB of
 * its own below it before `.bss`. */
_stack_start_cpu0 = 0x3FCAF000;

/* The frame that called `Reset` — the one the bootloader is standing in.
 *
 * `XtHart::new` leaves `WindowStart = 1`: **frame 0 is resident**. `PS_BOOT`
 * carries `CALLINC = 2` (the bootloader reaches the app entry through a
 * `callx8`), so `Reset`'s `entry a1, 0x10` creates frame 2 and leaves frame 0
 * resident with whatever `a1` it had. On silicon that is the bootloader's own
 * stack pointer. On a **direct load** it is zero, and the first thing that
 * spills every live window — `save_context`'s `SPILL_REGISTERS`, which runs on
 * every exception — takes `_WindowOverflow8` for frame 0 and executes
 *
 *     l32e a0, a1, -12     // a0 <- the caller's SP, out of THIS frame's save area
 *
 * against `a1 = 0`, i.e. `0xFFFFFFF4`. A load/store error raised inside a
 * window handler (`PS.EXCM = 1`) is an immediate DOUBLE EXCEPTION, and the run
 * never comes back. That is not hypothetical: it is what every fixture with an
 * exception in it did before these two symbols existed — one entry to
 * `VECBASE + 0x080`, then 1.6 million to `VECBASE + 0x3C0`.
 *
 * So the boot frame gets a stack, in two halves:
 *
 *  - the **host runner** seeds `a1 = _boot_frame_sp` before the first
 *    instruction, exactly as it seeds `PS`. A machine that seeds the
 *    bootloader's PS and not the bootloader's stack pointer has only half of
 *    "as the bootloader left it" (a finding for M3, which owns the real
 *    machine's seeding);
 *  - `mach::__pre_init` seeds `[_boot_frame_sp - 16 .. _boot_frame_sp)` with
 *    `{0, _boot_frame_caller_sp, 0, 0}` — a zero return address, which is the
 *    backtrace chain's documented terminator, and one more stack pointer so
 *    the `s32e a4..a7, a0, -32..-20` that follows the `l32e` lands somewhere
 *    legal.
 *
 * Both live in the 4 KiB above the stack, which nothing else claims. */
_boot_frame_sp = 0x3FCAF040;
_boot_frame_caller_sp = 0x3FCAF080;
