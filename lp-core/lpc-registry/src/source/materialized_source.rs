//! Materialized UTF-8 source text and effective version.

use alloc::string::String;

use lpc_model::Revision;

/// UTF-8 source text read transiently for compile or diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaterializedSource {
    pub version: Revision,
    pub text: String,
    pub diagnostic_name: String,
}
