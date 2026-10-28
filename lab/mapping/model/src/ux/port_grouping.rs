//! How an output card groups a port's pixels (D14, plan Q3): view state,
//! never an offer and never saved.
//!
//! - **Objects** (the default): each object's span is one run, and the
//!   pixels between them one run each, as lp2014 drew mapped shapes.
//! - **Groups of N**: the same, but the free pixels are cut into free
//!   segments of N, the size of the next object to patch, so each one is a
//!   place a hand patch can put it (lp2014's "Pixel Group Size").
//! - **Pixels**: only objects' spans are runs; the rest is plain pixels.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PortGrouping {
    #[default]
    Objects,
    Groups,
    Pixels,
}

impl PortGrouping {
    pub const ALL: [PortGrouping; 3] = [
        PortGrouping::Objects,
        PortGrouping::Groups,
        PortGrouping::Pixels,
    ];

    /// The word on the switch; groups say their size.
    pub fn words(self, size: u32) -> String {
        match self {
            PortGrouping::Objects => "segments".into(),
            PortGrouping::Groups => format!("groups of {size}"),
            PortGrouping::Pixels => "pixels".into(),
        }
    }
}
