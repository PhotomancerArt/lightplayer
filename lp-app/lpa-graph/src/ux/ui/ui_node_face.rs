//! What a node shows on the canvas (Studio's `face`), in card-local world
//! units: `(0, 0)` is the node's top-left. Dense data stays out: a fixture
//! face's lamps and an output face's pixel colours are read from the
//! editor by id.

use crate::domain::geom::{Rect, Vec2};
use crate::domain::node::NodeId;
use crate::domain::part_ref::TreeRef;
use crate::domain::pattern::Pattern;
use crate::ux::pixel_grid::PixelGrid;
use crate::ux::ui::Emphasis;

#[derive(Debug, Clone, PartialEq)]
pub enum UiNodeFace {
    Fixture(UiFixtureFace),
    Output(UiOutputFace),
    Source(UiSourceFace),
    Plain(UiPlainFace),
}

/// A fixture's face. While you work in it, it is the mapping editor: what's
/// drawn in it, its wiring, and the picture its lamps sample. Otherwise it
/// is Studio's live view: its lamps alone, each as a cell of light. Its
/// lamps are dense: read them by `fixture` from `Project::objects_in` and
/// colour them from the board.
#[derive(Debug, Clone, PartialEq)]
pub struct UiFixtureFace {
    pub fixture: NodeId,
    pub look: FixtureLook,
    /// Each object's line and marks, drawn while editing; their emphasis
    /// dims a fixture's lamps outside the level you are in either way.
    pub objects: Vec<UiObject>,
    /// The order the wire runs through its objects (`data_path.rs`); empty
    /// in the live view.
    pub wires: Vec<UiWire>,
    /// The render source whose picture its lamps sample, drawn faintly
    /// behind them where the product's fit puts it (`picture_rect`, the
    /// frame's [`BoardFrame::picture`](crate::BoardFrame::picture));
    /// `None` in the live view, or when nothing writes the channel it reads.
    pub samples: Option<NodeId>,
    /// The shape the picture is fitted into: the fixture's render size,
    /// else the picture's own.
    pub render_size: Option<(u32, u32)>,
    /// Why nothing is drawn in it: what it holds that the lab can't show
    /// yet, in a sentence.
    pub not_shown: Option<String>,
    /// How far apart its lamps are, in its own units: the usual gap
    /// between one lamp and the next along an object. Its lamps are drawn
    /// as a share of it, so they are as big as they can be without
    /// touching, however the fixture is spaced.
    pub pitch: f64,
}

/// How a fixture's face is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixtureLook {
    /// Lamps as cells of light, as Studio's live view draws them: the
    /// fixture as it looks lit.
    Live,
    /// The mapping editor: lines, where data enters, which way it runs,
    /// the wiring, lamps as dots. While it or anything in it is selected,
    /// or a drawing tool is out.
    Editing,
}

/// One object drawn in a fixture.
#[derive(Debug, Clone, PartialEq)]
pub struct UiObject {
    pub id: TreeRef,
    pub shape: UiShape,
    /// Lit when selected (or inside the selection); dimmed outside the
    /// level you are working in.
    pub emphasis: Emphasis,
    /// Where its data enters: lamp 0.
    pub first_lamp: Option<Vec2>,
    /// Which way its data runs, for a small arrow: a point on it and a
    /// direction.
    pub arrow: Option<(Vec2, Vec2)>,
    /// A path's jumpers, by segment of its line: wire with no lamps,
    /// drawn dashed.
    pub jumpers: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiShape {
    Polyline(Vec<Vec2>),
    Ring {
        center: Vec2,
        radius: f64,
    },
    /// A ring set's rings, outer first: one object.
    Rings {
        center: Vec2,
        radii: Vec<f64>,
    },
    /// A closed outline whose inside is the object: a grid's box, a filled
    /// polygon's outline.
    Outline(Vec<Vec2>),
}

/// A stretch of a fixture's wiring.
#[derive(Debug, Clone, PartialEq)]
pub enum UiWire {
    /// From one object's last lamp to the next one's first.
    Link { from: Vec2, to: Vec2 },
    /// Where a port's data comes in, and which way it runs on.
    Entry { at: Vec2, along: Vec2 },
}

/// An output's face: one row per port. Each row's pixels are colours from
/// the board, read by output and port key.
#[derive(Debug, Clone, PartialEq)]
pub struct UiOutputFace {
    pub output: NodeId,
    pub ports: Vec<UiPortRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UiPortRow {
    pub key: String,
    pub name: String,
    /// `50/60 px`.
    pub fill: String,
    /// The row on the card.
    pub row: Rect,
    /// The selection lands here.
    pub tinted: bool,
    /// Where each pixel's square sits; each is selectable.
    pub grid: PixelGrid,
    /// Each object's run of pixels, outlined.
    pub runs: Vec<UiRun>,
    /// Pixels ringed: the pixel of each selected lamp (D47), and while a
    /// patch is armed, the candidate in focus.
    pub marked: Vec<u32>,
    /// Where two segments share channels (plan D7; `port_rows.rs`): their
    /// outline, and the words for the row (`3 lamps overlap`), loud.
    pub overlaps: Vec<Rect>,
    pub overlap: Option<String>,
}

/// The outline round one run of a port's pixels: one box per line it
/// covers. Lit while its segment is hovered or selected; quiet when no
/// segment is patched there (a gap).
#[derive(Debug, Clone, PartialEq)]
pub struct UiRun {
    pub rects: Vec<Rect>,
    pub emphasis: Emphasis,
    /// What it is, in lamps: "ring r3 · 12 lamps", "gap · 16 lamps".
    pub label: String,
}

/// A shader or other render source: what it is and where it writes.
#[derive(Debug, Clone, PartialEq)]
pub struct UiSourceFace {
    /// The node, to find its picture in the frame by.
    pub source: NodeId,
    /// `shader`.
    pub kind: String,
    /// The channel it writes: `visual.out`.
    pub writes: String,
    /// The stand-in it shows instead of running its GLSL.
    pub pattern: Pattern,
}

/// A node the lab doesn't simulate: it says what it is.
#[derive(Debug, Clone, PartialEq)]
pub struct UiPlainFace {
    /// `power button`.
    pub kind: String,
}
