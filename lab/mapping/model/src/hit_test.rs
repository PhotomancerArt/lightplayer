//! What is under the cursor — always the DEEPEST thing (a lamp if one is
//! close enough, else the line or ring stroke). Which level a click actually
//! selects is `pick.rs`'s job.

use crate::camera::Camera;
use crate::fixture::Fixture;
use crate::geom::{Vec2, distance_to_segment};
use crate::object::ObjectShape;
use crate::target::Target;

/// How close, in screen pixels, the cursor must be to a lamp to hit it.
pub const LAMP_HIT_PX: f64 = 7.0;
/// How close, in screen pixels, to a line or ring stroke.
pub const STROKE_HIT_PX: f64 = 6.0;

pub fn hit(fixture: &Fixture, camera: &Camera, screen: Vec2) -> Option<Target> {
    let objects = fixture.objects();

    let mut best: Option<(f64, Target)> = None;
    for o in &objects {
        for (i, lamp) in o.lamps.iter().enumerate() {
            let d = camera.to_screen(lamp.pos).distance(screen);
            if d <= LAMP_HIT_PX && best.as_ref().is_none_or(|(bd, _)| d <= *bd) {
                best = Some((
                    d,
                    Target::Lamp {
                        object: o.id.clone(),
                        index: i as u32,
                    },
                ));
            }
        }
    }
    if let Some((_, t)) = best {
        return Some(t);
    }

    let world = camera.to_world(screen);
    for o in &objects {
        let d_world = match o.shape {
            ObjectShape::Segment { from, to } => distance_to_segment(world, from, to),
            ObjectShape::Ring { center, radius } => (world.distance(center) - radius).abs(),
        };
        let d = d_world * camera.scale;
        if d <= STROKE_HIT_PX && best.as_ref().is_none_or(|(bd, _)| d <= *bd) {
            best = Some((d, fixture.object_target(&o.id)));
        }
    }
    best.map(|(_, t)| t)
}
