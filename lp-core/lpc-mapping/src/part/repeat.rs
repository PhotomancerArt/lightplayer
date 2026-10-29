//! A repeat: copies of one part turned evenly round a centre.
//!
//! **A repeat holds a part with its own id** (F6), of any kind, a group
//! included. Copy 0 is that part, with its own ids (`panel`); copy k is the
//! same subtree with every id in copy k (`panel@k`) and every lamp turned
//! by today's arithmetic ([`RepeatShape::instance_degrees`], [`Rotation2d`]),
//! copy 0 included (by 0°), so the lamps are today's instances' bit for bit.
//! Copies are generated parts: never stored.
//!
//! [`Rotation2d`]: crate::Rotation2d

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::map2d_doc::{Map2dShape, PathAlign, PathShape, RepeatShape};
use crate::part::part::Part;

/// `count` copies of `part`, copy k turned `k · 360° / count` clockwise
/// about `center`.
#[derive(Debug, Clone, PartialEq)]
pub struct Repeat {
    pub part: Box<Part>,
    pub center: [f32; 2],
    pub count: u32,
}

impl Repeat {
    #[must_use]
    pub fn new(part: Part, center: [f32; 2], count: u32) -> Self {
        Self {
            part: Box::new(part),
            center,
            count,
        }
    }

    /// The degrees copy `k` is turned by, from today's
    /// [`RepeatShape::instance_degrees`] itself (not a copy of its
    /// arithmetic). For every copy, in order.
    #[must_use]
    pub fn copy_degrees(&self) -> Vec<f32> {
        // Today's repeat holds a shape; the angle reads only `count`, so an
        // empty path stands in for it.
        let today = RepeatShape {
            shape: Box::new(Map2dShape::Path(PathShape {
                points: Vec::new(),
                count: 0,
                reversed: false,
                gaps: Vec::new(),
                align: PathAlign::On,
            })),
            center: self.center,
            count: self.count,
        };
        (0..self.count).map(|k| today.instance_degrees(k)).collect()
    }
}
