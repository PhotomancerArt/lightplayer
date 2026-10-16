//! An arrow, said semantically: from here to there, leaving and arriving
//! this way, with these heads and these words on it.
//!
//! Nothing here is geometry yet. [`Arrow::layout`] (or
//! [`layout_arrows`](crate::layout_arrows) for several at once, so their
//! labels keep out of each other's way) turns it into an
//! [`ArrowGeometry`](crate::ArrowGeometry) a renderer only has to draw.

use crate::{
    ArrowGeometry, Cubic, EndDirection, Gutter, Head, Label, LayoutOptions, Path, Route, Segment,
    TextMeasure, Vec2, layout_arrows,
};

#[derive(Debug, Clone, PartialEq)]
pub struct Arrow {
    pub from: Vec2,
    pub to: Vec2,
    pub leave: EndDirection,
    pub arrive: EndDirection,
    pub route: Route,
    pub start_head: Option<Head>,
    pub end_head: Option<Head>,
    pub labels: Vec<Label>,
}

impl Arrow {
    /// A smooth arrow from `from` to `to`, leaving and arriving straight
    /// toward each other, with the default head at its end and no labels.
    pub fn new(from: Vec2, to: Vec2) -> Arrow {
        Arrow {
            from,
            to,
            leave: EndDirection::Direct,
            arrive: EndDirection::Direct,
            route: Route::default(),
            start_head: None,
            end_head: Some(Head::default()),
            labels: Vec::new(),
        }
    }

    pub fn straight(mut self) -> Arrow {
        self.route = Route::Straight;
        self
    }

    pub fn route(mut self, route: Route) -> Arrow {
        self.route = route;
        self
    }

    pub fn leave(mut self, d: EndDirection) -> Arrow {
        self.leave = d;
        self
    }

    pub fn arrive(mut self, d: EndDirection) -> Arrow {
        self.arrive = d;
        self
    }

    pub fn end_head(mut self, head: Option<Head>) -> Arrow {
        self.end_head = head;
        self
    }

    pub fn start_head(mut self, head: Option<Head>) -> Arrow {
        self.start_head = head;
        self
    }

    pub fn label(mut self, label: Label) -> Arrow {
        self.labels.push(label);
        self
    }

    /// The whole line, tip to tip, before any head trims it, for any route:
    /// what labels are measured along and what a hover should test.
    pub fn line(&self) -> Path {
        match self.route {
            Route::Gutter(g) => gutter_path(self.from, self.to, g),
            Route::Straight => Path::of_cubic(&self.centreline(), true),
            Route::Curve(_) => Path::of_cubic(&self.centreline(), false),
        }
    }

    /// The whole curve, tip to tip, for a straight or curved route. A
    /// gutter route is no one cubic: use [`Arrow::line`]; this gives the
    /// straight line between its ends.
    pub fn centreline(&self) -> Cubic {
        match self.route {
            Route::Straight | Route::Gutter(_) => Cubic::line(self.from, self.to),
            Route::Curve(bend) => {
                let reach = (self.from.distance(self.to) * bend.reach).max(bend.min);
                let out = self.leave.leaving(self.from, self.to);
                let inward = self.arrive.arriving(self.from, self.to);
                Cubic::new(
                    self.from,
                    self.from + out * reach,
                    self.to - inward * reach,
                    self.to,
                )
            }
        }
    }

    /// This arrow on its own, laid out.
    pub fn layout<M: TextMeasure + ?Sized>(
        &self,
        measure: &M,
        options: &LayoutOptions,
    ) -> ArrowGeometry {
        layout_arrows(core::slice::from_ref(self), measure, options)
            .pop()
            .expect("one arrow in, one geometry out")
    }
}

/// A gutter route from `a` to `b`: sideways to the lane, along it, and
/// sideways into `b`, each turn a quarter round (a cubic, as SVG draws it).
pub(crate) fn gutter_path(a: Vec2, b: Vec2, g: Gutter) -> Path {
    let lane = g.lane_x;
    let out = (lane - a.x).signum();
    let inward = (b.x - lane).signum();
    let down = (b.y - a.y).signum();
    let r = g
        .radius
        .min((lane - a.x).abs())
        .min((b.x - lane).abs())
        .min((b.y - a.y).abs() / 2.0)
        .max(0.0);
    // How far a quarter circle's handles reach, as a share of its radius.
    const K: f64 = 0.552_284_75;
    let mut segments = vec![Segment::MoveTo(a)];
    if r <= 0.0 || down == 0.0 {
        segments.push(Segment::LineTo(Vec2::new(lane, a.y)));
        segments.push(Segment::LineTo(Vec2::new(lane, b.y)));
        segments.push(Segment::LineTo(b));
        return Path { segments };
    }
    let turn1_in = Vec2::new(lane - out * r, a.y);
    let turn1_out = Vec2::new(lane, a.y + down * r);
    let turn2_in = Vec2::new(lane, b.y - down * r);
    let turn2_out = Vec2::new(lane + inward * r, b.y);
    segments.push(Segment::LineTo(turn1_in));
    segments.push(Segment::CubicTo {
        c1: turn1_in + Vec2::new(out * r * K, 0.0),
        c2: turn1_out - Vec2::new(0.0, down * r * K),
        to: turn1_out,
    });
    segments.push(Segment::LineTo(turn2_in));
    segments.push(Segment::CubicTo {
        c1: turn2_in + Vec2::new(0.0, down * r * K),
        c2: turn2_out - Vec2::new(inward * r * K, 0.0),
        to: turn2_out,
    });
    segments.push(Segment::LineTo(b));
    Path { segments }
}
