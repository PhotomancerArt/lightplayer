//! Power numbers for each [`LampType`].
//!
//! # What this is, and what it is not
//!
//! This is a **guardrail against the common mistake**, not a power model. Its
//! job is to stop a first-time project from browning out a USB-powered board in
//! a reboot loop. It is not accurate enough to size a supply, and it is not
//! trying to be — a real model needs lamps assigned to independent power
//! domains, since a single strip with power injected every few metres is
//! already several supplies, and that is its own piece of design work.
//!
//! Treat the numbers below as deliberately rough. They will be improved when
//! there is a bench rig to improve them with; until then, being roughly right
//! and on by default beats being precisely right and off.
//!
//! # These numbers are not measured
//!
//! Every preset here is [`PowerProvenance::Estimated`] — assembled from vendor
//! datasheets and community figures, not from a meter on our bench. They are
//! good enough to keep a project from browning out a board, and not good enough
//! to size a power supply to the last milliamp. Anything shown to a user must
//! say "estimated".
//!
//! # Replacing them with real numbers
//!
//! The intended path is an on-device `test_power` harness alongside the existing
//! `test_gpio_calibrate` / `test_dither` harnesses in `fw-esp32c6`: sweep LED
//! count against brightness, log measured draw, and write the results back here
//! with [`PowerProvenance::Measured`]. That work is deliberately out of scope
//! for the plan that introduced this module.
//!
//! # Editing
//!
//! This is a plain `const` table on purpose — no data file, no parser, no
//! shipped asset, and nothing to keep in sync at runtime. Correcting a number
//! is a one-line edit here.

use super::lamp_type::LampType;
use super::power_model::PowerModel;

/// How much to trust a preset's numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerProvenance {
    /// From datasheets and community figures. Not measured by us.
    Estimated,
    /// Measured on a bench with a known load.
    Measured,
}

/// A lamp type's power behaviour and how much to trust it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LampPreset {
    pub model: PowerModel,
    pub provenance: PowerProvenance,
}

/// WS2812B and compatible 5V per-pixel parts.
///
/// Datasheet figures put a fully-lit pixel near 60 mA at 5V, split evenly
/// across three dies. Quiescent draw of the controller is around 1 mA.
const WS2812B_5V: LampPreset = LampPreset {
    model: PowerModel::LinearPerChannel {
        ma_per_channel_full: 20,
        ma_idle_per_led: 1,
    },
    provenance: PowerProvenance::Estimated,
};

/// WS2815, 12V per-pixel with constant-current drivers.
///
/// Running at 12V lets the part deliver comparable light at roughly a third of
/// the current. The quiescent term is proportionally larger than on 5V parts —
/// the backup-data circuit draws whether or not the pixel is lit — which is why
/// it must not be dropped from the estimate.
const WS2815_12V: LampPreset = LampPreset {
    model: PowerModel::LinearPerChannel {
        ma_per_channel_full: 6,
        ma_idle_per_led: 1,
    },
    provenance: PowerProvenance::Estimated,
};

/// WS2811 12V strips: one addressable chip drives three LEDs in series.
///
/// The chip's constant-current output feeds the whole group, so a group of
/// three draws roughly what one channel draws — the extra LEDs cost voltage
/// headroom, not current. Treating these as three independent pixels
/// over-estimates draw by about 3x.
const WS2811_12V: LampPreset = LampPreset {
    model: PowerModel::SeriesGroup {
        ma_per_channel_full: 20,
        ma_idle_per_led: 1,
        leds_per_group: 3,
    },
    provenance: PowerProvenance::Estimated,
};

/// The preset for a lamp type.
pub const fn preset_for(lamp: LampType) -> LampPreset {
    match lamp {
        LampType::Ws2812b5v => WS2812B_5V,
        LampType::Ws281512v => WS2815_12V,
        LampType::Ws281112v => WS2811_12V,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_lamp_type_has_a_preset() {
        for lamp in LampType::ALL {
            let preset = preset_for(*lamp);
            let (full, idle) = match preset.model {
                PowerModel::LinearPerChannel {
                    ma_per_channel_full,
                    ma_idle_per_led,
                } => (ma_per_channel_full, ma_idle_per_led),
                PowerModel::SeriesGroup {
                    ma_per_channel_full,
                    ma_idle_per_led,
                    ..
                } => (ma_per_channel_full, ma_idle_per_led),
            };
            assert!(full > 0, "{} needs a channel figure", lamp.as_str());
            assert!(idle > 0, "{} needs an idle figure", lamp.as_str());
        }
    }

    #[test]
    fn nothing_claims_to_be_measured_yet() {
        for lamp in LampType::ALL {
            assert_eq!(
                preset_for(*lamp).provenance,
                PowerProvenance::Estimated,
                "{} claims measurement we have not done",
                lamp.as_str()
            );
        }
    }

    #[test]
    fn series_parts_declare_a_group_larger_than_one() {
        let PowerModel::SeriesGroup { leds_per_group, .. } = preset_for(LampType::Ws281112v).model
        else {
            panic!("ws2811 is a series part");
        };
        assert!(leds_per_group > 1);
    }
}
