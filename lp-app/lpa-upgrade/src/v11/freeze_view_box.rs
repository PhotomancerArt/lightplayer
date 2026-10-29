//! The view box of a format 1–5 map2d that has no `canvas`: its lamps' own
//! bounds, the box today's fit frames when a document gives none.
//!
//! Format 6's `view_box` is required (fixture-space theory, D7, D11). A
//! document with a `canvas` keeps it as its view box; one without is fitted
//! today by `lpc_mapping::fit_points_in_place` against
//! `bounds_of_points` of its rendered lamps. Written as the view box, those
//! same bounds give the fit the same frame, so every lamp reaches the
//! sampler where it did, bit for bit.
//!
//! **The one place the step reads the current model.** The bounds are of
//! the lamps the format-6 document renders (`lpc_mapping::part::render`),
//! read back from the very text the step writes: the format-6 render is
//! the one the engine fits, and it lands today's lamps bit for bit
//! (`tests/step_v11_to_v12.rs`, `positions_kept`). Freezing a copy of the
//! renderer here instead would be a second copy of every shape's
//! arithmetic for one rarely-taken branch (no committed project takes it;
//! Studio's old starter fixture, a canvas-less format-1 grid, does).

use lpc_mapping::bounds_of_points;
use lpc_mapping::map2d6;
use lpc_mapping::part::render;

use super::v11_files::refused;
use crate::json::JsonNode;
use crate::upgrade_error::UpgradeError;

/// `[min_x, min_y, width, height]` of the lamps `document` (a format-6
/// document the step built) renders. A document with no lamps gets a box
/// of no size: nothing is fitted, as today.
pub(crate) fn lamp_bounds(file: &str, document: &JsonNode) -> Result<[f32; 4], UpgradeError> {
    let text = String::from_utf8(document.to_pretty_bytes())
        .map_err(|_| refused(file, "its format-6 document is not text"))?;
    let mapping = map2d6::read(&text).map_err(|error| {
        refused(
            file,
            &format!("its format-6 document does not read back ({error})"),
        )
    })?;
    let tree = render(&mapping).map_err(|error| {
        refused(
            file,
            &format!("its format-6 document does not render ({error})"),
        )
    })?;
    Ok(bounds_of_points(tree.positions())
        .map_or([0.0; 4], |b| [b.min_x, b.min_y, b.width, b.height]))
}
