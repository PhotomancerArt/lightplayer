//! A point or a direction, in whatever units the caller draws in.
//!
//! The library never assumes pixels: an arrow laid out in world units comes
//! back in world units. Like the screen and SVG, y grows downward, so a
//! positive angle turns clockwise.

use core::ops::{Add, Mul, Neg, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2::new(0.0, 0.0);

    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// The unit vector at `degrees` (0 = +x, 90 = +y, which is down).
    pub fn from_angle_deg(degrees: f64) -> Vec2 {
        let r = degrees.to_radians();
        Vec2::new(r.cos(), r.sin())
    }

    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    pub fn distance(self, other: Vec2) -> f64 {
        (self - other).length()
    }

    pub fn dot(self, o: Vec2) -> f64 {
        self.x * o.x + self.y * o.y
    }

    /// The z of the 3D cross product: positive when `o` turns clockwise
    /// from `self` (y down).
    pub fn cross(self, o: Vec2) -> f64 {
        self.x * o.y - self.y * o.x
    }

    pub fn lerp(self, other: Vec2, t: f64) -> Vec2 {
        self + (other - self) * t
    }

    /// The same direction at length 1; the zero vector stays zero rather
    /// than turning into NaN.
    pub fn normalized(self) -> Vec2 {
        let l = self.length();
        if l > 0.0 {
            self * (1.0 / l)
        } else {
            Vec2::ZERO
        }
    }

    /// Turned a quarter clockwise (y down): `(1, 0)` becomes `(0, 1)`.
    pub fn perp(self) -> Vec2 {
        Vec2::new(-self.y, self.x)
    }

    /// Its angle in degrees, in (−180, 180].
    pub fn angle_deg(self) -> f64 {
        self.y.atan2(self.x).to_degrees()
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: f64) -> Vec2 {
        Vec2::new(self.x * k, self.y * k)
    }
}

impl Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2::new(-self.x, -self.y)
    }
}
