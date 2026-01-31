//! Core types for test generation.

/// Vector type (base type without dimension).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VecType {
    Vec,
    IVec,
    UVec,
    BVec,
}

/// Vector dimension (width).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    D2,
    D3,
    D4,
}

impl Dimension {
    pub fn as_usize(self) -> usize {
        match self {
            Dimension::D2 => 2,
            Dimension::D3 => 3,
            Dimension::D4 => 4,
        }
    }
}
