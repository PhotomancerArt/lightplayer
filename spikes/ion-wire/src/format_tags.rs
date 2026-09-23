//! LPBJ tag bytes (see the table in the crate docs).

pub const UINT_INLINE_MAX: u8 = 0x3F;
pub const VDICT_INLINE_BASE: u8 = 0x40;
pub const VDICT_INLINE_COUNT: usize = 64;
pub const STR_INLINE_BASE: u8 = 0x80;
pub const STR_INLINE_MAX_LEN: usize = 31;
pub const OBJECT: u8 = 0xA0;
pub const ARRAY: u8 = 0xA1;
pub const ARRAY_END: u8 = 0xA2;
pub const NULL: u8 = 0xA3;
pub const FALSE: u8 = 0xA4;
pub const TRUE: u8 = 0xA5;
pub const UINT: u8 = 0xA6;
pub const NEG_INT: u8 = 0xA7;
pub const DECIMAL_POS: u8 = 0xA8;
pub const DECIMAL_NEG: u8 = 0xA9;
pub const STRING: u8 = 0xAA;
pub const BLOB: u8 = 0xAB;
pub const VDICT: u8 = 0xAC;
pub const BACKREF: u8 = 0xAD;
pub const NUMBER_TEXT: u8 = 0xAE;

pub const KDICT_INLINE_COUNT: usize = 0xF0;
pub const KDICT_WIDE_BASE: u8 = 0xF0;
pub const KDICT_WIDE_MAX: u8 = 0xFB;
pub const KEY_INLINE: u8 = 0xFC;
pub const KEY_BACKREF: u8 = 0xFD;
pub const OBJECT_END: u8 = 0xFF;
