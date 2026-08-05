//! Conversion pinning tests (M3 deliverable #3): hand-checked expected
//! values for the numeric conversion from each source format into a
//! [`lpc_model::Gradient`], for at least 3 palettes.
//!
//! **Deviation from the phase file**: a true WLED-render image comparison
//! (rendering the original FastLED/WLED palette on real firmware or a WLED
//! simulator and diffing pixels) is infeasible in this environment — no
//! WLED build, no reference renderer. Instead these tests pin the
//! *stop-list conversion* numerically: every `(at, c)` value below was
//! computed by hand from the verified upstream source (FastLED's
//! `crgb.h` named-color hex table / `colorpalettes.cpp.hpp`; the cpt-city
//! `.cpt` files fetched directly from
//! <https://phillips.shef.ac.uk/pub/cpt-city/>), independent of the
//! `gen_palettes.py` script that produced the checked-in JSON — so a
//! transcription bug in the generator is exactly what this test would
//! catch. See `assets/palettes/third-party/COPYING.md` for the source
//! URLs.

use lpa_palettes::palette_by_id;

const EPS: f32 = 0.0005;

fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < EPS,
        "{what}: expected {expected}, got {actual}"
    );
}

fn assert_color_close(actual: [f32; 3], expected: [f32; 3], what: &str) {
    for (channel, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_close(*a, *e, &format!("{what} channel {channel}"));
    }
}

/// FastLED's `OceanColors_p` (`CRGBPalette16`), MIT,
/// <https://github.com/FastLED/FastLED/blob/master/src/colorpalettes.cpp.hpp>.
/// 16 named colors, hex from `src/fl/gfx/crgb.h`; FastLED reads a
/// `CRGBPalette16` with linear interpolation across the full range, so this
/// imports as 16 stops evenly spaced across `[0, 1]`.
#[test]
fn fastled_ocean_pins_to_hand_computed_named_color_hex() {
    let palette = palette_by_id("fastled_ocean").expect("fastled_ocean in catalog");

    assert_eq!(palette.gradient.stops.len(), 16);
    assert_eq!(palette.license.as_ref().unwrap().spdx, "MIT");

    // stop 0: MidnightBlue = 0x191970
    assert_close(palette.gradient.stops[0].at, 0.0, "stop 0 at");
    assert_color_close(
        palette.gradient.stops[0].c,
        [
            0x19 as f32 / 255.0,
            0x19 as f32 / 255.0,
            0x70 as f32 / 255.0,
        ],
        "stop 0 (MidnightBlue)",
    );

    // stop 5: MediumBlue = 0x0000CD, position 5/15
    assert_close(palette.gradient.stops[5].at, 5.0 / 15.0, "stop 5 at");
    assert_color_close(
        palette.gradient.stops[5].c,
        [0.0, 0.0, 0xCD as f32 / 255.0],
        "stop 5 (MediumBlue)",
    );

    // stop 12: Aquamarine = 0x7FFFD4
    assert_close(palette.gradient.stops[12].at, 12.0 / 15.0, "stop 12 at");
    assert_color_close(
        palette.gradient.stops[12].c,
        [
            0x7F as f32 / 255.0,
            0xFF as f32 / 255.0,
            0xD4 as f32 / 255.0,
        ],
        "stop 12 (Aquamarine)",
    );

    // stop 15 (last): LightSkyBlue = 0x87CEFA, position 1.0
    assert_close(palette.gradient.stops[15].at, 1.0, "stop 15 at");
    assert_color_close(
        palette.gradient.stops[15].c,
        [
            0x87 as f32 / 255.0,
            0xCE as f32 / 255.0,
            0xFA as f32 / 255.0,
        ],
        "stop 15 (LightSkyBlue)",
    );
}

/// cpt-city `jjg/misc/rainfall`, Public Domain (J.J. Green, 2004),
/// <https://phillips.shef.ac.uk/pub/cpt-city/jjg/misc/rainfall>. The source
/// `.cpt` is "discrete" (flat 20-unit bands over a 0-140 domain, hard edges)
/// -> `InterpMethod::Step`, one stop per band start, position normalized
/// against the full 0-140 domain (so a 20-unit band is 20/140 = 1/7 wide).
#[test]
fn jjg_misc_rainfall_pins_to_hand_computed_cpt_bands() {
    let palette = palette_by_id("jjg_misc_rainfall").expect("jjg_misc_rainfall in catalog");

    assert_eq!(palette.gradient.stops.len(), 7);
    assert_eq!(palette.gradient.method, lpc_model::InterpMethod::Step);
    assert_eq!(palette.license.as_ref().unwrap().spdx, "Public Domain");

    // Band 0: 0-20 of 140 -> 229/180/44
    assert_close(palette.gradient.stops[0].at, 0.0, "stop 0 at");
    assert_color_close(
        palette.gradient.stops[0].c,
        [229.0 / 255.0, 180.0 / 255.0, 44.0 / 255.0],
        "stop 0",
    );

    // Band 3: starts at 60/140 -> 145/206/126
    assert_close(palette.gradient.stops[3].at, 60.0 / 140.0, "stop 3 at");
    assert_color_close(
        palette.gradient.stops[3].c,
        [145.0 / 255.0, 206.0 / 255.0, 126.0 / 255.0],
        "stop 3",
    );

    // Band 6 (last): starts at 120/140 -> 6/155/66
    assert_close(palette.gradient.stops[6].at, 120.0 / 140.0, "stop 6 at");
    assert_color_close(
        palette.gradient.stops[6].c,
        [6.0 / 255.0, 155.0 / 255.0, 66.0 / 255.0],
        "stop 6",
    );
}

/// cpt-city `bhw/bhw1/bhw1_13`, CC-BY-3.0 (Blackheartedwolf, 2011),
/// <https://phillips.shef.ac.uk/pub/cpt-city/bhw/bhw1/bhw1_13>. The
/// simplest continuous case: a single-row PSP gradient, two stops,
/// `InterpMethod::Linear`.
#[test]
fn bhw_bhw1_13_pins_to_hand_computed_two_stop_ramp() {
    let palette = palette_by_id("bhw_bhw1_13").expect("bhw_bhw1_13 in catalog");

    assert_eq!(palette.gradient.stops.len(), 2);
    assert_eq!(palette.gradient.method, lpc_model::InterpMethod::Linear);
    assert_eq!(palette.license.as_ref().unwrap().spdx, "CC-BY-3.0");

    assert_close(palette.gradient.stops[0].at, 0.0, "stop 0 at");
    assert_color_close(
        palette.gradient.stops[0].c,
        [255.0 / 255.0, 255.0 / 255.0, 128.0 / 255.0],
        "stop 0",
    );
    assert_close(palette.gradient.stops[1].at, 1.0, "stop 1 at");
    assert_color_close(
        palette.gradient.stops[1].c,
        [212.0 / 255.0, 130.0 / 255.0, 230.0 / 255.0],
        "stop 1",
    );
}
