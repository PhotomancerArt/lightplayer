//! Conversion between frontend SourceLocId and Cranelift SourceLoc.
//!
//! Keeps Cranelift coupling in codegen; the frontend SourceLocManager uses only SourceLocId.

use cranelift_codegen::ir::SourceLoc;
use lp_glsl_frontend::src_loc_manager::SourceLocId;

/// Convert SourceLocId to Cranelift SourceLoc for use with builder.set_srcloc(), etc.
pub fn to_cranelift_srcloc(id: SourceLocId) -> SourceLoc {
    SourceLoc::new(id.bits())
}

/// Convert Cranelift SourceLoc to SourceLocId for lookup in SourceLocManager.
pub fn from_cranelift_srcloc(srcloc: SourceLoc) -> SourceLocId {
    SourceLocId::from_bits(srcloc.bits())
}
