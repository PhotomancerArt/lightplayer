//! The canvas, as core emits it for one view state.

use crate::domain::geom::Vec2;
use crate::domain::notice::Notice;
use crate::ux::hints::HintBar;
use crate::ux::ui::{
    UiCanvasEdge, UiCanvasNode, UiInspector, UiMenu, UiOverlay, UiToolbar, UiTree,
};

/// Everything the page draws, in one tree. No positions: the page lays the
/// nodes out (`layout_input.rs`) and places the cards itself.
#[derive(Debug, Clone, PartialEq)]
pub struct UiCanvasView {
    pub layout: LayoutKind,
    pub size_class: SizeClass,
    /// Every node, flat, in tree order. Nesting (a module around its
    /// children) is each node's `parent`, never nested components, so a
    /// card is the same component in every layout.
    pub nodes: Vec<UiCanvasNode>,
    pub edges: Vec<UiCanvasEdge>,
    /// What's drawn over the cards at one size on screen: outlines,
    /// handles, the selection box, a drawing in progress.
    pub overlay: UiOverlay,
    pub inspector: UiInspector,
    pub tree: UiTree,
    pub hints: HintBar,
    pub notice: Option<Notice>,
    pub toolbar: UiToolbar,
    /// The right-click list, while it is open.
    pub menu: Option<UiMenu>,
    /// The note beside the cursor.
    pub cursor_note: Option<UiCursorNote>,
    /// What the pointer looks like over the canvas.
    pub cursor: UiCursor,
}

/// How big the screen is, as Studio's breakpoints (Tailwind's) name it:
/// a phone, a tablet or narrow window, a desk (architecture §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SizeClass {
    Sm,
    Md,
    #[default]
    Lg,
}

impl SizeClass {
    /// The class of a canvas `px` wide: sm below 640, md below 1024.
    pub fn of_width(px: f64) -> SizeClass {
        if px < 640.0 {
            SizeClass::Sm
        } else if px < 1024.0 {
            SizeClass::Md
        } else {
            SizeClass::Lg
        }
    }

    /// The layout a screen this size starts in: the stack on a phone.
    pub fn default_layout(self) -> LayoutKind {
        match self {
            SizeClass::Sm => LayoutKind::Stack,
            SizeClass::Md | SizeClass::Lg => LayoutKind::Canvas,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            SizeClass::Sm => "sm",
            SizeClass::Md => "md",
            SizeClass::Lg => "lg",
        }
    }
}

/// Which layout the canvas is in (architecture §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LayoutKind {
    /// Automatic, pan and zoom.
    #[default]
    Canvas,
    /// One column in tree order; it scrolls.
    Stack,
}

/// The note beside the cursor, and where the cursor is, in screen pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct UiCursorNote {
    pub text: String,
    pub at: Vec2,
}

/// What the pointer looks like, from what it would do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiCursor {
    #[default]
    Default,
    /// Space held: ready to pan.
    Grab,
    /// Panning.
    Grabbing,
    /// Moving the selection.
    Move,
    /// A move that moves nothing.
    NotAllowed,
    /// A drawing tool.
    Crosshair,
    /// Over a handle that resizes left–right, up–down, or both.
    ResizeX,
    ResizeY,
    ResizeXY,
}
