//! Drawing on the picture: a finger on a connected board's live picture
//! writes a touch set the board's pattern reads (plan
//! `drawing-on-the-picture`, D11–D13).
//!
//! - [`lamp_picture_frame`]: where a lamp field is drawn and where a point
//!   on it is in pattern space — the one place both directions live, shared
//!   with the web's lamp renderer;
//! - [`draw_target`]: whether a picture is drawable and where its touches
//!   go, read off the board's binding graph;
//! - [`stroke_controller`]: reported fingers in, paced momentary writes out;
//! - [`stroke_feel`]: every number a stroke's feel hangs on;
//! - the view's facts ([`UiDrawable`], [`UiDrawSurface`], [`UiDrawTarget`])
//!   and the web's report ([`UiStroke`]).

pub mod draw_target;
pub mod lamp_picture_frame;
pub mod stroke_controller;
pub mod stroke_feel;
mod ui_draw_surface;
mod ui_draw_target;
mod ui_drawable;
mod ui_stroke;

pub use draw_target::touches_draw_target;
pub use lamp_picture_frame::{
    LAMP_CELL_INSET_KEEP, LAMP_CELL_PITCH_FRACTION, LampPictureFrame, PictureFit,
    lamp_footprint_floor, lamp_hint_position,
};
pub use stroke_controller::StrokeController;
pub use stroke_feel::{
    STROKE_FEED_GAP, STROKE_FEED_LINGER, STROKE_RENEW_INTERVAL, STROKE_SEND_INTERVAL,
    STROKE_TTL_MS, TOUCH_ID_WRAP,
};
pub use ui_draw_surface::UiDrawSurface;
pub use ui_draw_target::UiDrawTarget;
pub use ui_drawable::UiDrawable;
pub use ui_stroke::{UiStroke, UiStrokePointer};
