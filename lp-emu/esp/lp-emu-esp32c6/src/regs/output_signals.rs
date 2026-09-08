// GPIO matrix **output signal** numbers, transcribed from esp-rs/esp-hal:
//   esp-metadata-generated-0.4.0/src/_generated_esp32c6.rs:5210-5211
//   (`OutputSignal::RMT_SIG_0 = 71`, `RMT_SIG_1 = 72`)
// and from esp-rs/esp-pacs:
//   esp32c6-0.23.2/src/gpio/func_out_sel_cfg.rs:22 — "s=128: output of
//   GPIO\[n\] equals GPIO_OUT_REG\[n\]".
// Repositories: https://github.com/esp-rs/esp-hal, https://github.com/esp-rs/esp-pacs
// Both MIT OR Apache-2.0; MIT text vendored at licenses/esp-pacs-MIT.txt.
//
// HAND-WRITTEN, unlike its neighbours: `scripts/emu/pac-regnames.py` reads
// register blocks, and this is neither a register block nor in the PAC — it
// is the interconnect's signal enumeration, which lives in esp-hal's
// metadata. Only the signals the pin trace has to *name* are here; anything
// else prints as `sig<N>` (see `Gpio::signal_name`), which is honest about
// the table being partial rather than pretending an unknown number is
// unroutable.

/// `func_out_sel_cfg[n].out_sel` values this crate can name, `(number, name)`,
/// sorted by number.
pub static OUTPUT_SIGNALS: &[(u16, &str)] = &[
    (71, "RMT_SIG_0"),
    (72, "RMT_SIG_1"),
    (128, "GPIO_OUT"),
];

/// `out_sel == 128`: the pad follows `GPIO_OUT_REG[n]` rather than any
/// peripheral. The reset value of `func_out_sel_cfg[n]` is `0x80`, so every
/// pad starts here.
pub const OUT_SEL_GPIO: u16 = 128;

/// `OutputSignal::RMT_SIG_0`. Channel `ch`'s signal is `RMT_SIG_0 + ch`.
pub const RMT_SIG_0: u16 = 71;

/// The name for an `out_sel` value, or `None` when the table does not carry
/// it.
pub fn output_signal_name(sel: u16) -> Option<&'static str> {
    OUTPUT_SIGNALS
        .binary_search_by_key(&sel, |(n, _)| *n)
        .map(|i| OUTPUT_SIGNALS[i].1)
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_and_names_the_signals_the_trace_uses() {
        assert!(OUTPUT_SIGNALS.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(output_signal_name(RMT_SIG_0), Some("RMT_SIG_0"));
        assert_eq!(output_signal_name(RMT_SIG_0 + 1), Some("RMT_SIG_1"));
        assert_eq!(output_signal_name(OUT_SEL_GPIO), Some("GPIO_OUT"));
        assert_eq!(output_signal_name(0), None, "unknown ids print as sig<N>");
    }
}
