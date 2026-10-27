//! One segment per path, or one per subpath.

use crate::edit::edit_error::EditError;
use crate::edit::part_place::part_mut;
use crate::part::{Mapping, PartId, PartKind, PathSplit, Shape};

/// Make the path `id` render one segment, or one per subpath (`path1/s1`,
/// `path1/s2`…). Overrides stay as they are: one on the path's own node
/// (`path1`) is on its one segment when whole, and on the part above its
/// subpaths when split (its direction then the default for each, F14);
/// overrides on subpaths are gone while the path is whole, reported, and
/// found again when it splits.
pub fn set_path_split(m: &mut Mapping, id: &PartId, split: PathSplit) -> Result<(), EditError> {
    let part = part_mut(m, id).ok_or_else(|| EditError::NoSuchPart(id.clone()))?;
    match &mut part.kind {
        PartKind::Shape(Shape::Path(path)) => {
            path.split = split;
            Ok(())
        }
        _ => Err(EditError::NotAPath(id.clone())),
    }
}
