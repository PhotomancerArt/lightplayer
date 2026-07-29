//! Import authored sources into mapping documents.
//!
//! Import is an explicit *conversion*, not a runtime source of truth: the
//! output is a [`crate::Map2dDoc`] the user owns and edits from then on.

mod svg_data;
mod svg_error;
mod svg_group;
mod svg_import;
mod svg_parser;

pub use svg_error::SvgImportError;
pub use svg_import::svg_to_doc;
