//! SourceFileRef resolution and UTF-8 materialization from artifacts.

pub mod materialize;
mod resolve;
mod source_file_ref;

pub use materialize::MaterializedSource;
pub use materialize::{MaterializeError, SourceDiagnosticCtx, materialize_source};
pub use resolve::{ResolveError, resolve_source_file};
pub use source_file_ref::SourceFileRef;
