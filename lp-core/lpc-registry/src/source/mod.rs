//! SourceFileRef resolution and UTF-8 materialization from artifacts.

mod materialize;
mod materialized_source;
mod resolve;
mod source_file_ref;

pub use materialize::{MaterializeError, SourceDiagnosticCtx, materialize_source};
pub use materialized_source::MaterializedSource;
pub use resolve::{ResolveError, resolve_source_file};
pub use source_file_ref::SourceFileRef;
