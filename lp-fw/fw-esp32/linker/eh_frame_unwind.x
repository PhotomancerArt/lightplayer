/* Symbols for the `unwinding` crate's fde-static feature.
 *
 * __executable_start / __etext: PC range the unwinder checks.
 * __eh_frame: provided by the patched esp-hal eh_frame.x (see build.rs).
 *
 * __etext is set to end of ROM (conservative; the unwinder just uses it
 * to validate that a PC falls within the binary's code range). */
PROVIDE(__executable_start = ORIGIN(ROTEXT));
PROVIDE(__etext = ORIGIN(ROTEXT) + LENGTH(ROTEXT));
