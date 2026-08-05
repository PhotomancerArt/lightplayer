//! The gradient wire-size guard: a `GradientConfig`'s `LpValue` storage
//! form must fit a project-read frame with room to spare.
//!
//! The padded §5 recipe measured ~17.7 KiB on the wire — larger than the
//! whole `PROJECT_READ_FRAME_MAX_BYTES` (16 KiB) frame budget by itself, so
//! any event echoing one (the binding-graph probe's channel values after a
//! palette pick, a def slot-root snapshot after a slot-local pick) failed
//! the entire project read. The count-bounded storage form is what keeps a
//! realistic palette pick a small fraction of a frame; this test pins that.

use lpc_model::{Colorspace, Gradient, GradientConfig, GradientStop, InterpMethod, ToLpValue};
use lpc_wire::budget::PROJECT_READ_FRAME_MAX_BYTES;

fn ramp(stops: usize) -> Gradient {
    Gradient {
        space: Colorspace::Oklab,
        method: InterpMethod::Linear,
        stops: (0..stops)
            .map(|i| GradientStop {
                at: i as f32 / (stops - 1).max(1) as f32,
                c: [0.9, 0.1, i as f32 * 0.01],
            })
            .collect(),
    }
}

#[test]
fn a_realistic_gradient_config_is_a_small_fraction_of_a_frame() {
    // The chooser's common cases: a catalog palette held, and a cycle of a
    // few members. Both must leave the frame plenty of room for the rest of
    // the event (the binding graph, the def around the slot).
    for (label, config, max_bytes) in [
        (
            "default static",
            GradientConfig::default(),
            PROJECT_READ_FRAME_MAX_BYTES / 16,
        ),
        (
            "held 8-stop catalog palette",
            GradientConfig::Static(ramp(8)),
            PROJECT_READ_FRAME_MAX_BYTES / 8,
        ),
        (
            "cycle of 4 x 8 stops",
            GradientConfig::Cycle {
                set: vec![ramp(8); 4],
                step_seconds: 20.0,
                fade_seconds: 0.5,
            },
            PROJECT_READ_FRAME_MAX_BYTES / 2,
        ),
    ] {
        let len = lpc_wire::ser_write_json_len(&config.to_lp_value());
        assert!(
            len <= max_bytes,
            "{label}: wire LpValue JSON is {len} bytes, budget-fraction cap is {max_bytes}"
        );
    }
}

#[test]
fn a_full_cycle_of_wled_scale_imports_fits_one_frame() {
    // The heaviest config the import path actually produces: WLED gradients
    // carry up to 18 stops (the reason MAX_GRADIENT_STOPS is 24), and a
    // cycle holds up to 8 of them. This must fit a frame ALONE; the padded
    // form failed this for every config, of any size.
    //
    // The maximal LEGAL config — 8 members × 24 authored stops each — still
    // exceeds one frame (~21 KiB) and needs event chunking to sync; that
    // known bound is registered in
    // docs/debt/maximal-gradient-cycle-exceeds-frame.md.
    let config = GradientConfig::Cycle {
        set: vec![ramp(18); 8],
        step_seconds: 20.0,
        fade_seconds: 0.5,
    };
    let len = lpc_wire::ser_write_json_len(&config.to_lp_value());
    assert!(
        len <= PROJECT_READ_FRAME_MAX_BYTES,
        "8 x 18-stop cycle: wire LpValue JSON is {len} bytes, frame budget is {PROJECT_READ_FRAME_MAX_BYTES}"
    );
}
