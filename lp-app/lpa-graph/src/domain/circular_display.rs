//! The circular display: Yona's common case, built in code (G1's lead
//! journey, plan P5). One fixture, one circle of rings — the innermost of
//! radius 0, a single lamp at the centre — on one output with five ports.
//!
//! [`Project::circular_display_wired`] is the same display as it is really
//! wired, for the hardware preview: ring 3 soldered counter-clockwise, and
//! ring 3, ring 4 and the centre on D1 — what the journey fixes by hand.
//! The output has five ports, so one ring per port is a choice too.

use lpc_mapping::part::{Circle, PartKind, Shape, TreeId};
use lpc_mapping::patching::Direction;

use crate::domain::geom::Vec2;
use crate::domain::part_ref::TreeRef;
use crate::domain::pattern::Pattern;
use crate::domain::project::Project;

/// The rings, outside in: (radius, lamps). The last is the centre lamp,
/// at radius 0.
pub const RINGS: [(f32, u32); 5] = [(42.0, 24), (32.0, 16), (22.0, 12), (12.0, 8), (0.0, 1)];

impl Project {
    /// The circular display, starting at main D0's first pixel, its rings
    /// end to end, outside in.
    pub fn circular_display() -> Self {
        let mut p = Project::new("circular display");
        p.add_shader(None, "rainbow", Pattern::Rainbow);
        let display = p
            .add_fixture_sized(None, "display", Vec2::new(100.0, 100.0))
            .expect("at the root");
        let main = p
            .add_output(
                None,
                "main",
                &[("D0", 64), ("D1", 64), ("D2", 64), ("D3", 64), ("D4", 64)],
            )
            .expect("at the root");
        p.add(
            &display,
            PartKind::Shape(Shape::Circle(Circle::new([50.0, 50.0], &RINGS))),
        );
        p.start_at(&display, &main, "0", 0);
        p
    }

    /// The circular display as it is really wired: ring 3 runs
    /// counter-clockwise on the wire, and ring 3, ring 4 and the centre
    /// are on D1, from its first pixel.
    pub fn circular_display_wired() -> Self {
        let mut p = Project::circular_display();
        let main = p.named("main").map(|n| n.id.clone()).expect("main");
        let display = p.named("display").map(|n| n.id.clone()).expect("display");
        let r3 = TreeRef::new(&display, TreeId::parse("circle1/r3").expect("an id"));
        p.patch_segment(&r3, &main, "1", 0);
        p.edit_override(&r3, |o| o.direction = Some(Direction::Reverse));
        p
    }
}
