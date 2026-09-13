// GPIO matrix **output signal** numbers for the ESP32-S3, transcribed from
// esp-rs/esp-hal:
//   esp-metadata-generated-0.4.0/src/_generated_esp32s3.rs:4801-4804
//   (`OutputSignal::RMT_SIG_0 = 81` … `RMT_SIG_3 = 84`), :4761
//   (`OutputSignal::U0TXD = 12`), :4923 (`OutputSignal::GPIO = 256`)
// and its **input** twin, read independently and deliberately kept apart:
//   esp-metadata-generated-0.4.0/src/_generated_esp32s3.rs:4628-4631
//   (`InputSignal::RMT_SIG_0 = 81` … `RMT_SIG_3 = 84`), :4573
//   (`InputSignal::U0RXD = 12`)
// plus esp-rs/esp-pacs:
//   esp32s3-0.35.2/src/gpio/func_out_sel_cfg.rs — `out_sel` is nine bits
//   (0:8), "s=0-255: output of GPIO[n] equals input of peripheral[s]. s=256:
//   output of GPIO[n] equals GPIO_OUT_REG[n]", and the register resets to
//   `0x0100` = 256.
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
// ⚠️ **The two enumerations agree on this chip, and that is recorded as a
// coincidence, not as an identity.** `OutputSignal::RMT_SIG_0` is 81 and
// `InputSignal::RMT_SIG_0` is 81 — but on the *classic* they are 87 and 83,
// four apart, and a table that had assumed one enumeration there would
// mislabel every RMT trace line without failing anything (see
// `lp-emu-esp32v3/src/regs/output_signals.rs`). So the two spaces stay two
// constants and two tables here, read from two places in the metadata, and
// neither lookup falls back to the other.

/// `func_out_sel_cfg[n].out_sel` values this crate can name, `(number, name)`,
/// sorted by number.
///
/// `RMT_SIG_0..3` are 81..=84: TX channel `ch` drives `RMT_SIG_0 + ch`
/// ([`RMT_SIG_0`]). `U0TXD` is the mask ROM's console, which the ROM leaves on
/// its IO_MUX function rather than routing through the matrix — it is here so
/// that a pad someone *did* route to it reads as itself.
pub static OUTPUT_SIGNALS: &[(u16, &str)] = &[
    (12, "U0TXD"),
    (81, "RMT_SIG_0"),
    (82, "RMT_SIG_1"),
    (83, "RMT_SIG_2"),
    (84, "RMT_SIG_3"),
    (256, "GPIO_OUT"),
];

/// `out_sel == 256`: the pad follows `GPIO_OUT_REG[n]` rather than any
/// peripheral ([`crate::periph::gpio::OUT_SEL_GPIO`], the constant the view
/// itself compares against).
///
/// **Established, not guessed** — three independent readings agree:
///
/// 1. the PAC's field doc, *"s=0-255: output of GPIO\[n\] equals input of
///    peripheral\[s\]. s=256: output of GPIO\[n\] equals GPIO_OUT_REG\[n\]"*
///    (`esp32s3-0.35.2/src/gpio/func_out_sel_cfg.rs`);
/// 2. that register's **reset value**, `0x0100` = 256 — every pad comes out
///    of reset following `GPIO_OUT`, which is only coherent if 256 is the
///    selector;
/// 3. `OutputSignal::GPIO = 256`
///    (`esp-metadata-generated-0.4.0/src/_generated_esp32s3.rs:4923`).
///
/// It is **not** the C6's 128: `out_sel` is nine bits here (0:8) against the
/// C6's eight, and 128 is a real peripheral signal on this chip. Getting it
/// wrong routes every plain output pad to the wrong source and the symptom is
/// a dead strip.
pub const OUT_SEL_GPIO: u16 = 256;

/// `OutputSignal::RMT_SIG_0` — **81**. TX channel `ch`'s signal is
/// `RMT_SIG_0 + ch`, which is what [`crate::periph::rmt`] drives onto the
/// fabric.
pub const RMT_SIG_0: u16 = 81;

/// `func_in_sel_cfg[s]` signal numbers this crate can name, `(number, name)`,
/// sorted by number — the **input** half of the matrix.
///
/// The S3's four RX channels are accept-and-warn (P07 models no RMT RX on
/// this chip), so only the console's input and the RMT's four receive signals
/// are named; the entries exist so that the two enumerations are visibly
/// separate tables rather than one table a reader might assume covers both
/// directions.
pub static INPUT_SIGNALS: &[(u16, &str)] = &[
    (12, "U0RXD"),
    (81, "RMT_SIG_0"),
    (82, "RMT_SIG_1"),
    (83, "RMT_SIG_2"),
    (84, "RMT_SIG_3"),
];

/// `InputSignal::RMT_SIG_0` on this chip — **81**, which happens to be the
/// same number as [`RMT_SIG_0`].
///
/// A separate constant on purpose. The classic's two enumerations are four
/// apart (87 out, 83 in) and the C6's are not the S3's either; recording this
/// chip's coincidence as an identity would be a bug waiting for the next
/// part, so the RX side reads this and the TX side reads [`RMT_SIG_0`].
pub const RMT_RX_SIG_0: u16 = 81;

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
    fn the_tables_are_sorted_and_name_the_signals_the_trace_uses() {
        assert!(OUTPUT_SIGNALS.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(INPUT_SIGNALS.windows(2).all(|w| w[0].0 < w[1].0));
        const NAMES: [&str; 4] = ["RMT_SIG_0", "RMT_SIG_1", "RMT_SIG_2", "RMT_SIG_3"];
        for ch in 0..4u16 {
            assert_eq!(
                output_signal_name(RMT_SIG_0 + ch),
                Some(NAMES[ch as usize]),
                "TX channel {ch} drives RMT_SIG_0 + {ch}"
            );
        }
        assert_eq!(output_signal_name(OUT_SEL_GPIO), Some("GPIO_OUT"));
        assert_eq!(output_signal_name(0), None, "unknown ids print as sig<N>");
    }

    /// The two enumerations coincide here and that is a fact about this chip,
    /// not a rule: the assertion is written so that a future edit which
    /// collapsed the two constants into one would have to delete this test.
    #[test]
    fn the_output_and_input_enumerations_are_two_tables_that_happen_to_agree() {
        assert_eq!(RMT_SIG_0, 81);
        assert_eq!(RMT_RX_SIG_0, 81);
        // The classic's are 87 and 83. The S3's coincide; recorded, not relied on.
        assert_eq!(input_signal_name(12), Some("U0RXD"));
        assert_eq!(output_signal_name(12), Some("U0TXD"));
        assert_eq!(
            input_signal_name(OUT_SEL_GPIO),
            None,
            "256 is an out_sel sentinel and not an input signal"
        );
    }

    /// The pad constant is the chip's, and it is not the C6's.
    #[test]
    fn out_sel_gpio_is_256_and_agrees_with_the_gpio_view() {
        assert_eq!(OUT_SEL_GPIO, 256);
        assert_eq!(OUT_SEL_GPIO, crate::periph::gpio::OUT_SEL_GPIO);
        assert_ne!(OUT_SEL_GPIO, 128, "128 is a real signal on this chip");
    }
}
