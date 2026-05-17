use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LvaluePath {
    pub base: LvalueBase,
    pub projections: Vec<LvalueProjection>,
}

impl LvaluePath {
    pub fn new(base: LvalueBase) -> Self {
        Self {
            base,
            projections: Vec::new(),
        }
    }

    pub fn with_projection(mut self, projection: LvalueProjection) -> Self {
        self.projections.push(projection);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LvalueBase {
    Local(usize),
    Param(usize),
    Global(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LvalueProjection {
    Swizzle(Vec<SwizzleComponent>),
    Field(String),
    Index,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwizzleComponent {
    X,
    Y,
    Z,
    W,
}
