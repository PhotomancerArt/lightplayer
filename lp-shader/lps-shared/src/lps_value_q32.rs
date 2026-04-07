use alloc::boxed::Box;
use lps_q32::Q32;

pub enum LpsValueQ32 {
    I32(i32),
    U32(u32),
    F32(Q32),
    Bool(bool),
    Vec2([Q32; 2]),
    Vec3([Q32; 3]),
    Vec4([Q32; 4]),
    IVec2([i32; 2]),
    IVec3([i32; 3]),
    IVec4([i32; 4]),
    UVec2([u32; 2]),
    UVec3([u32; 3]),
    UVec4([u32; 4]),
    BVec2([bool; 2]),
    BVec3([bool; 3]),
    BVec4([bool; 4]),
    Mat2x2([[Q32; 2]; 2]), // [[col0_row0, col0_row1], [col1_row0, col1_row1]]
    Mat3x3([[Q32; 3]; 3]), // [[col0_row0, col0_row1, col0_row2], [col1_row0, ...], ...]
    Mat4x4([[Q32; 4]; 4]), // [[col0_row0, col0_row1, col0_row2, col0_row3], [col1_row0, ...], ...]
    /// Fixed-size array; elements use the same recursive shape (scalars, vectors, matrices, nested arrays).
    Array(Box<[crate::LpsValueQ32]>),
    /// Struct instance; `fields` are in declaration order (names match [`StructMember::name`] when present).
    Struct {
        name: Option<alloc::string::String>,
        fields: alloc::vec::Vec<(alloc::string::String, crate::LpsValueQ32)>,
    },
}
