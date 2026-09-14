//! The block shapes, which are `lp-emu-jit`'s with this crate's decoded form
//! in them.
//!
//! Nothing here is Xtensa's except the type parameter: a block is a
//! straight-line run of guest instructions, a block set is what one module
//! holds, and both are the ABI's (XD6 made them generic for exactly this).

/// One straight-line run of Xtensa instructions.
pub type Block = lp_emu_jit::blocks::Block<crate::decode::Decoded>;

/// The blocks one wasm module holds, and how to find one by pc.
pub type BlockSet = lp_emu_jit::blocks::BlockSet<crate::decode::Decoded>;

pub use lp_emu_jit::blocks::BlockEnd;

/// The most instructions one translated block may hold.
///
/// The same 64 the RV32 side uses and the same 64
/// `lp_xt_emu::mach::block::MAX_BLOCK_SLOTS` uses — a bound on the emitter's
/// working set, not an architectural fact. A block that would be longer is
/// split, and the second half is a block start like any other.
pub const MAX_BLOCK_INSTS: usize = lp_emu_jit::blocks::MAX_BLOCK_INSTS;
