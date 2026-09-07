/* Pin the per-lamp render path's flash code to the head of `.text`.
 *
 * A GNU ld `--section-ordering-file` (binutils >= 2.43; the esp toolchain's
 * xtensa-esp32-elf-ld is 2.43.1): every input section listed here is placed
 * at the START of the named output section, in this order, ahead of what
 * esp-hal's text.x collects with `*(.literal .text .literal.* .text.*)`.
 * Passed from build.rs as `-Wl,--section-ordering-file=<this file>`.
 *
 * WHY: each ESP32 core's flash cache is 32 KB, two-way set-associative,
 * 32-byte blocks (TRM 1.3.4) — 512 sets, 16 KB per way, set = (addr>>5)&0x1FF
 * — and IROM (.text) and DROM (.rodata) share it. The per-lamp working set
 * (the JIT's scalar builtin trampolines, their libm / __divsf3 callees, the
 * sample loop, the direct-lamp encode) was scattered across ~1.7 MB of .text,
 * so whether any three of its lines shared a set depended on where unrelated
 * code had pushed each function: an unrelated change moved
 * `projects/test/zook-dome-1500`'s frame across 50–56 ms. A contiguous run of
 * at most 32 KB touches every set at most twice and cannot evict itself; only
 * its position relative to the rest of the working set (GAMMA16 in DROM,
 * above all) still varies, and that residual is what the sweep in
 * `probes/flash-layout/` quotes as the noise floor.
 *
 * WHY NOT a separate output section: the classic's bootloader maps exactly
 * one IROM and one DROM segment ("Image contains multiple IROM segments.
 * Only the last one will be mapped." → IllegalInstruction). Anything pinned
 * must stay inside `.text`, which is what an ordering file does and an
 * `INSERT BEFORE .text` script cannot.
 *
 * Input-section globs, not object globs (LTO merges the image into one
 * codegen unit; only the section name — derived from the mangled symbol —
 * survives), the same rule as rwdata_hook.x. `.literal.*` travels with its
 * function: an Xtensa function's `l32r` constants are their own input
 * section. Patterns are substrings of v0-mangled names, so `*fixture_node*`
 * matches `_RNv…12fixture_node…`.
 *
 * Check with `probes/flash-layout/cache-sets.py` after changing the list:
 * the cluster must stay under 32 KB (it is ~20 KB) and every hot function
 * must land inside it.
 */
.text : {
  /* Scalar shader builtins the JIT calls per sample: flash trampolines
   * (entry + l32r + callx8) and the callees they reach. */
  *(.literal.__lp_lpir_* .text.__lp_lpir_*)
  *(.literal.*compiler_builtins*float*div* .text.*compiler_builtins*float*div*)
  *(.literal.*__divsf3* .text.*__divsf3*)
  *(.literal.*libm* .text.*libm*)
  /* The per-batch sampling loop between the fixture and the JIT. */
  *(.literal.*sample_request*VisualSampleStream* .text.*sample_request*VisualSampleStream*)
  *(.literal.*lpvm_shader*LpvmShader* .text.*lpvm_shader*LpvmShader*)
  *(.literal.*px_shader*BackendAdapter* .text.*px_shader*BackendAdapter*)
  *(.literal.*shader_node*sample_visual_into* .text.*shader_node*sample_visual_into*)
  /* The per-lamp encode: coordinate fill, direct sampling, gamma,
   * brightness, the control-buffer write. */
  *(.literal.*fixture_node*DirectCoordFill* .text.*fixture_node*DirectCoordFill*)
  *(.literal.*control_render_target*ControlRenderTarget* .text.*control_render_target*ControlRenderTarget*)
  *(.literal.*fixture_node*stream_direct_lamps* .text.*fixture_node*stream_direct_lamps*)
  *(.literal.*fixture_node*write_direct_lamps* .text.*fixture_node*write_direct_lamps*)
  *(.literal.*fixture_node*encode_fixture_channel* .text.*fixture_node*encode_fixture_channel*)
  *(.literal.*fixture_node*render_direct_fixture_control* .text.*fixture_node*render_direct_fixture_control*)
}
