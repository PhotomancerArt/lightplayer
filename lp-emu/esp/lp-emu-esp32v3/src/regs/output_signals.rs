// GPIO matrix **output signal** numbers for the classic ESP32, transcribed
// from esp-rs/esp-hal:
//   esp-metadata-generated-0.4.0/src/_generated_esp32.rs:4428
//   (`OutputSignal::RMT_SIG_0 = 87` … `RMT_SIG_7 = 94`, `U0TXD = 14`)
// and its **input** twin, deliberately kept apart:
//   esp-metadata-generated-0.4.0/src/_generated_esp32.rs:4255
//   (`InputSignal::RMT_SIG_0 = 83` … `RMT_SIG_7 = 90`, `U0RXD = 14`)
// plus esp-rs/esp-pacs:
//   esp32-0.40.2/src/gpio/func_out_sel_cfg.rs — `out_sel` is nine bits,
//   "select one of the 256 output to 40 GPIO", and 256 means "the pad follows
//   GPIO_OUT_REG[n]".
// Repositories: https://github.com/esp-rs/esp-hal, https://github.com/esp-rs/esp-pacs
// Both MIT OR Apache-2.0; MIT text vendored at licenses/esp-pacs-MIT.txt.
//
// HAND-WRITTEN, unlike its neighbours: `scripts/emu/pac-regnames.py` reads
// register blocks, and this is neither a register block nor in the PAC — it
// is the interconnect's signal enumeration, which lives in esp-hal's
// metadata. Only the signals a pin trace or a frame record has to *name* are
// here; anything else prints as `sig<N>` (see [`output_signal_name`]), which
// is honest about the table being partial rather than pretending an unknown
// number is unroutable.
//
// ⚠️ **The two enumerations do not agree on this chip, and mixing them
// mislabels a trace line without failing anything.** `OutputSignal::RMT_SIG_0`
// is **87**; `InputSignal::RMT_SIG_0` is **83**, so **87 is `RMT_SIG_4` on the
// input side**. `func_out_sel_cfg[pad].out_sel` is an *output* number and
// `func_in_sel_cfg[sig].in_sel` an *input* one, so the two tables below are
// separate and neither function falls back to the other.

/// `func_out_sel_cfg[n].out_sel` values this crate can name, `(number, name)`,
/// sorted by number.
///
/// `RMT_SIG_0..7` are 87..=94: TX channel `ch` drives `RMT_SIG_0 + ch`
/// ([`RMT_SIG_0`]). `U0TXD` is the console, which the ROM and esp-hal leave on
/// its IO_MUX function rather than routing through the matrix — it is here so
/// that a pad someone *did* route to it reads as itself.
pub static OUTPUT_SIGNALS: &[(u16, &str)] = &[
    (14, "U0TXD"),
    (87, "RMT_SIG_0"),
    (88, "RMT_SIG_1"),
    (89, "RMT_SIG_2"),
    (90, "RMT_SIG_3"),
    (91, "RMT_SIG_4"),
    (92, "RMT_SIG_5"),
    (93, "RMT_SIG_6"),
    (94, "RMT_SIG_7"),
    (256, "GPIO_OUT"),
];

/// `out_sel == 256`: the pad follows `GPIO_OUT_REG[n]` rather than any
/// peripheral (`crate::periph::gpio::OUT_SEL_GPIO`, which is the constant the
/// view itself compares against). **The C6's is 128**, which on this chip is a
/// real peripheral signal — the number is per chip and this file is the
/// classic's.
pub const OUT_SEL_GPIO: u16 = 256;

/// `OutputSignal::RMT_SIG_0`. TX channel `ch`'s signal is `RMT_SIG_0 + ch`,
/// which is what `crate::periph::rmt` drives onto the fabric.
pub const RMT_SIG_0: u16 = 87;

/// `func_in_sel_cfg[s]` signal numbers this crate can name, `(number, name)`,
/// sorted by number — the **input** half of the matrix.
///
/// The classic plan models no RMT RX, so only the console's input is named;
/// the entry exists so that the two enumerations are visibly separate tables
/// rather than one table a reader might assume covers both directions.
pub static INPUT_SIGNALS: &[(u16, &str)] = &[(14, "U0RXD")];

/// `InputSignal::RMT_SIG_0` on this chip — **83**, four below the output
/// table's 87. Not used by anything here (there is no RMT RX in this plan);
/// it is a constant so that the difference is written down where a future
/// reader of [`OUTPUT_SIGNALS`] will meet it.
pub const RMT_SIG_0_IN: u16 = 83;

/// The name for a `func_in_sel_cfg` signal number, or `None` when the table
/// does not carry it.
pub fn input_signal_name(sel: u16) -> Option<&'static str> {
    INPUT_SIGNALS
        .binary_search_by_key(&sel, |(n, _)| *n)
        .map(|i| INPUT_SIGNALS[i].1)
        .ok()
}

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
        assert!(INPUT_SIGNALS.windows(2).all(|w| w[0].0 < w[1].0));
        for ch in 0..8u16 {
            assert_eq!(
                output_signal_name(RMT_SIG_0 + ch),
                Some(alloc_name(ch)),
                "TX channel {ch} drives RMT_SIG_0 + {ch}"
            );
        }
        assert_eq!(output_signal_name(OUT_SEL_GPIO), Some("GPIO_OUT"));
        assert_eq!(output_signal_name(0), None, "unknown ids print as sig<N>");
    }

    fn alloc_name(ch: u16) -> &'static str {
        const NAMES: [&str; 8] = [
            "RMT_SIG_0",
            "RMT_SIG_1",
            "RMT_SIG_2",
            "RMT_SIG_3",
            "RMT_SIG_4",
            "RMT_SIG_5",
            "RMT_SIG_6",
            "RMT_SIG_7",
        ];
        NAMES[ch as usize]
    }

    /// The trap the file header names: 87 is `RMT_SIG_0` going out and
    /// `RMT_SIG_4` coming in. A table that mixed them would label a trace line
    /// with the wrong channel and nothing would fail — so the numbers are
    /// asserted apart.
    #[test]
    fn the_output_and_input_enumerations_are_not_the_same_numbers() {
        assert_eq!(RMT_SIG_0, 87);
        assert_eq!(RMT_SIG_0_IN, 83);
        assert_eq!(
            RMT_SIG_0_IN + 4,
            RMT_SIG_0,
            "87 is RMT_SIG_4 on the input side"
        );
        assert_eq!(input_signal_name(RMT_SIG_0), None);
        assert_eq!(input_signal_name(14), Some("U0RXD"));
        assert_eq!(output_signal_name(14), Some("U0TXD"));
    }

    /// The pad constant is the chip's, and it is not the C6's.
    #[test]
    fn out_sel_gpio_is_the_classics_256_and_agrees_with_the_gpio_view() {
        assert_eq!(OUT_SEL_GPIO, 256);
        assert_eq!(OUT_SEL_GPIO, crate::periph::gpio::OUT_SEL_GPIO);
        assert_ne!(OUT_SEL_GPIO, 128, "128 is a real signal on this chip");
    }
}
