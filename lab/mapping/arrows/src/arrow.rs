//! An arrow, said semantically: from here to there, leaving and arriving
//! this way, with these heads and these words on it.
//!
//! Nothing here is geometry yet. [`Arrow::layout`] (or
//! [`layout_arrows`](crate::layout_arrows) for several at once, so their
//! labels keep out of each other's way) turns it into an
//! [`ArrowGeometry`](crate::ArrowGeometry) a renderer only has to draw.

use crate::{
    ArrowGeometry, Cubic, EndDirection, Head, Label, LayoutOptions, Route, TextMeasure, Vec2,
    layout_arrows,
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

    /// The whole curve, tip to tip, before any head trims it: what labels
    /// are measured along and what a hover should test.
    pub fn centreline(&self) -> Cubic {
        match self.route {
            Route::Straight => Cubic::line(self.from, self.to),
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
