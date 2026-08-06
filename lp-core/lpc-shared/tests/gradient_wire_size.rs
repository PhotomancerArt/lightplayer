//! The gradient wire-size guard: a `GradientConfig`'s `LpValue` storage
//! form must fit a project-read frame with room to spare.
//!
//! History in two acts (ADR 2026-08-05-gradient-stops-string-storage):
//! the original always-padded §5 recipe measured ~17.7 KiB on the wire —
//! larger than the whole `PROJECT_READ_FRAME_MAX_BYTES` (16 KiB) budget by
//! itself, so any event echoing one (the binding-graph probe's channel
//! values after a palette pick, a def slot-root snapshot after a
//! slot-local pick) failed the entire project read. The count-bounded
//! interim form fixed realistic configs but a maximal 8×24-authored cycle
//! (~21 KiB) still could not ride one frame. The stops-string form closes
//! that too: metadata as structure, the payload as one compact literal.
//! This test pins all of it.

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
            PROJECT_READ_FRAME_MAX_BYTES / 32,
        ),
        (
            "held 8-stop catalog palette",
            GradientConfig::Static(ramp(8)),
            PROJECT_READ_FRAME_MAX_BYTES / 16,
        ),
        (
            "cycle of 4 x 8 stops",
            GradientConfig::Cycle {
                set: vec![ramp(8); 4],
                step_seconds: 20.0,
                fade_seconds: 0.5,
            },
            PROJECT_READ_FRAME_MAX_BYTES / 4,
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
fn even_the_maximal_legal_cycle_fits_one_frame_with_headroom() {
    // The worst LEGAL config: 8 members × 24 authored stops, every
    // component an arbitrary decimal (Oklab — hex never applies). This is
    // the case the count-bounded form still lost (~21 KiB > one frame,
    // formerly docs/debt/maximal-gradient-cycle-exceeds-frame.md); the
    // stops literal must land it well under budget so the probe event
    // carrying it alongside the rest of the graph still fits.
    let config = GradientConfig::Cycle {
        set: vec![ramp(24); 8],
        step_seconds: 20.0,
        fade_seconds: 0.5,
    };
    let len = lpc_wire::ser_write_json_len(&config.to_lp_value());
    assert!(
        len <= PROJECT_READ_FRAME_MAX_BYTES / 2,
        "maximal 8x24 cycle: wire LpValue JSON is {len} bytes, cap is {}",
        PROJECT_READ_FRAME_MAX_BYTES / 2
    );
}
