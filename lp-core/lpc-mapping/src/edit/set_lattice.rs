//! A lattice's numbers: a grid's columns, rows, spacing and routing, a
//! filled polygon's spacing, angle and routing.

use alloc::format;
use alloc::string::String;

use crate::GridRouting;
use crate::edit::edit_error::EditError;
use crate::edit::part_place::part_mut;
use crate::part::{Mapping, PartId, PartKind, Shape};

/// What to change in a lattice: each field `None` stays as it is.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LatticeEdit {
    /// A grid's columns.
    pub columns: Option<u32>,
    /// A grid's rows.
    pub rows: Option<u32>,
    /// Lamp to lamp, both ways.
    pub pitch: Option<f32>,
    /// A filled polygon's lattice, turned inside its outline, in degrees.
    pub angle_deg: Option<f32>,
    /// Rows back and forth (snake), or all the same way (raster).
    pub routing: Option<GridRouting>,
}

/// Change the grid or filled polygon `id` as `change` says. Its lamps are
/// laid again; its id and its overrides don't change. Refused whole, with
/// nothing changed: a part that is neither, no columns or rows, a spacing
/// that isn't above 0, an angle that isn't a number, and a field the shape
/// doesn't have (a filled polygon's lamps come from its outline, so it has
/// no columns or rows; a grid isn't turned).
pub fn set_lattice(m: &mut Mapping, id: &PartId, change: &LatticeEdit) -> Result<(), EditError> {
    let bad = |why: String| Err(EditError::BadLattice(why));
    if change.columns == Some(0) || change.rows == Some(0) {
        return bad(format!("{id} has at least one column and one row"));
    }
    if let Some(pitch) = change.pitch
        && (!pitch.is_finite() || pitch <= 0.0)
    {
        return bad(format!("{id}'s spacing is more than 0, not {pitch}"));
    }
    if change.angle_deg.is_some_and(|a| !a.is_finite()) {
        return bad(format!("{id}'s angle is a number of degrees"));
    }
    let part = part_mut(m, id).ok_or_else(|| EditError::NoSuchPart(id.clone()))?;
    match &mut part.kind {
        PartKind::Shape(Shape::Grid(grid)) => {
            if change.angle_deg.is_some() {
                return bad(format!("{id} is a grid, which isn't turned"));
            }
            grid.cols = change.columns.unwrap_or(grid.cols);
            grid.rows = change.rows.unwrap_or(grid.rows);
            grid.pitch = change.pitch.unwrap_or(grid.pitch);
            grid.routing = change.routing.unwrap_or(grid.routing);
        }
        PartKind::Shape(Shape::FilledPolygon(fill)) => {
            if change.columns.is_some() || change.rows.is_some() {
                return bad(format!(
                    "{id} is a filled polygon: its lamps come from its outline, not columns and rows"
                ));
            }
            fill.pitch = change.pitch.unwrap_or(fill.pitch);
            fill.angle_deg = change.angle_deg.unwrap_or(fill.angle_deg);
            fill.routing = change.routing.unwrap_or(fill.routing);
        }
        _ => return Err(EditError::NotALattice(id.clone())),
    }
    Ok(())
}
