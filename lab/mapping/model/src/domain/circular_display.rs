//! The circular display: Yona's common case, built in code (G1's lead
//! journey, plan P5). One fixture, one circle of rings — the innermost a
//! single lamp at the centre — on one output with two ports.
//!
//! It is built here rather than opened from files because the product's
//! files can't hold it yet: a circle saves as one ring object per ring,
//! and those open back as separate circles (findings.md).
//!
//! [`Project::circular_display_wired`] is the same display as it is really
//! wired, for the hardware preview: ring 3 soldered counter-clockwise, and
//! ring 3, ring 4 and the centre on D1 — what the journey fixes by hand.
//! The output has five ports, so one ring per port is a choice too.

use crate::domain::component::{ComponentId, ComponentKind};
use crate::domain::geom::Vec2;
use crate::domain::object::ObjectId;
use crate::domain::object_override::Direction;
use crate::domain::pattern::Pattern;
use crate::domain::project::Project;

/// The rings, outside in: (radius, lamps). The last is the centre lamp.
pub const RINGS: [(f64, u32); 5] = [(42.0, 24), (32.0, 16), (22.0, 12), (12.0, 8), (4.0, 1)];

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
            ComponentKind::circle(Vec2::new(50.0, 50.0), &RINGS),
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
        let r3 = ObjectId::new(&ComponentId::new("circle1"), "r3");
        p.patch_object(&r3, &main, "1", 0);
        p.edit_override(&r3, |o| o.direction = Some(Direction::Reversed));
        p
    }
}
