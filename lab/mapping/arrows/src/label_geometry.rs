//! A label, laid out: its words and emphasis, and where it went.

use crate::{LabelFit, LabelRole, PlacedLabel};

#[derive(Debug, Clone, PartialEq)]
pub struct LabelGeometry {
    pub text: String,
    pub role: LabelRole,
    pub font_size: f64,
    pub fit: LabelFit,
    /// `None` exactly when `fit` is `Dropped`.
    pub placed: Option<PlacedLabel>,
}
