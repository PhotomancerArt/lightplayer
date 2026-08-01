//! Types every backend shares.
//!
//! These are deliberately ISA-neutral and live here rather than inside one
//! backend, so that each ISA module can be compiled independently (see the
//! `isa-rv32` / `isa-xt` features). They previously lived in `isa/rv32/`,
//! whose comments already described them as "ISA-neutral even though RV32
//! owns the struct today" — landing the Xtensa backend made that literal.

use alloc::string::String;
use alloc::vec::Vec;

/// Byte offset in `.text` where a relocation applies.
#[derive(Clone, Debug)]
pub struct NativeReloc {
    pub offset: usize,
    pub symbol: String,
}

/// Raw machine code for one function plus relocations and debug info
/// (internal hand-off to [`crate::emit::EmittedCode`]).
///
/// Every backend fills this same shape; the bytes inside are whatever that
/// ISA emits.
#[derive(Clone, Debug)]
pub(crate) struct IsaEmitOutput {
    /// Machine code bytes.
    pub code: Vec<u8>,
    /// Call relocations (RV32: auipc+jalr pairs; Xtensa: literal-pool slots).
    pub relocs: Vec<NativeReloc>,
    /// Debug line table: (code_offset, optional_src_op).
    pub debug_lines: Vec<(u32, Option<u32>)>,
}

/// Options for annotated disassembly text.
#[derive(Clone, Copy, Debug, Default)]
pub struct DisasmOptions {
    /// Prefix each line with a 4-digit hex offset (function-local).
    pub show_hex_offset: bool,
}
