//! `lp-cli pattern preview` smoke: the catalog's plainest pattern, `pulse`,
//! rendered on the `strip60` swatch for a few frames, must light every lamp
//! and move — the whole preview chain (pattern read → swatch rig → host
//! engine → output samples → record) in one assertion.
//!
//! ```bash
//! cargo test -p lp-cli --test pattern_preview_smoke
//! ```

use std::path::{Path, PathBuf};

use lp_cli::commands::pattern::handler::preview_one;
use lp_cli::commands::pattern::pattern_source::PatternSource;
use lp_cli::commands::pattern::swatch::Swatch;

#[test]
fn pulse_on_strip60_renders_lit_moving_frames() {
    let workspace = workspace_dir();
    let pattern =
        PatternSource::read(&workspace.join("catalog/patterns/pulse")).expect("read pulse");
    let swatch =
        Swatch::read(&workspace.join("scripts/pattern-review/swatches/strip60.map2d.json"))
            .expect("read strip60");
    assert_eq!(swatch.lamp_count(), 60);

    let (record, frames) = preview_one(&pattern, &swatch, &[], 0.5, 24).expect("render");

    assert_eq!(frames.frame_count, 12);
    assert_eq!(frames.rgb.len(), 12 * 60 * 3);
    // Pulse's floor is 15 % of full, never black: every lamp of every frame
    // carries light, and the breathe moves frame to frame.
    for (frame, lamps) in frames.rgb.chunks(60 * 3).enumerate() {
        for (lamp, rgb) in lamps.chunks(3).enumerate() {
            assert!(
                rgb.iter().any(|v| *v > 0),
                "frame {frame} lamp {lamp} is black: {rgb:?}"
            );
        }
    }
    assert!(frames.peak() > 0);
    assert!(frames.changing_frames() > 0, "pulse did not animate");

    assert_eq!(record["error"], serde_json::Value::Null);
    assert_eq!(record["swatch"]["lamp_count"], 60);
    assert_eq!(record["pattern"]["slug"], "pulse");
    assert!(
        record["frames"].as_str().is_some_and(|b64| !b64.is_empty()),
        "frames are base64 text"
    );
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("lp-cli lives one level under the workspace root")
        .to_path_buf()
}
