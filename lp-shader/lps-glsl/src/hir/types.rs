use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use lp_collection::VecMap;

use lps_shared::{LpsModuleSig, LpsType, ParamQualifier, TextureBindingSpec};

use super::arena::{ExprId, ExprList, HirArena, PlaceId, WritebackList};
use crate::Span;
use crate::body::{BinaryOp, IncDecOp, UnaryOp};

#[derive(Debug, Clone)]
pub struct HirModule {
    pub functions: Vec<HirFunction>,
    pub meta: LpsModuleSig,
    pub uniforms: VecMap<String, UniformInfo>,
    pub globals: VecMap<String, GlobalInfo>,
    pub imports: Vec<ImportInfo>,
    pub texture_specs: VecMap<String, TextureBindingSpec>,
    pub texel_fetch_bounds: lpir::TexelFetchBoundsMode,
}

#[derive(Debug, Clone)]
pub struct UniformInfo {
    pub ty: LpsType,
    pub byte_offset: u32,
}

#[derive(Debug, Clone)]
pub struct GlobalInfo {
    pub ty: LpsType,
    pub byte_offset: u32,
}

#[derive(Debug, Clone)]
pub struct ImportInfo {
    pub key: ImportKey,
    pub module_name: String,
    pub func_name: String,
    pub param_types: Vec<lpir::IrType>,
    pub return_types: Vec<lpir::IrType>,
    pub lpfn_glsl_params: Option<String>,
    pub sret: bool,
}

pub(super) type StructTypes = VecMap<String, LpsType>;

/// Index of an import in [`HirModule::imports`] — what a call node carries
/// instead of the owning [`ImportKey`] (two `String`s) it used to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportId(pub u32);

impl ImportId {
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// The lanes an rvalue swizzle or field access selects, without a heap
/// vector: a field is always a contiguous run (any width), a swizzle picks
/// at most four lanes in any order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwizzleLanes {
    Contiguous { start: u16, len: u16 },
    Picked { lanes: [u8; 4], len: u8 },
}

impl SwizzleLanes {
    /// `None` when the selection is neither contiguous nor at most four
    /// lanes (no GLSL access produces that).
    pub fn from_slice(lanes: &[usize]) -> Option<Self> {
        let contiguous = lanes
            .first()
            .is_some_and(|first| lanes.iter().enumerate().all(|(i, l)| *l == first + i));
        if contiguous || lanes.is_empty() {
            let start = u16::try_from(lanes.first().copied().unwrap_or(0)).ok()?;
            let len = u16::try_from(lanes.len()).ok()?;
            return Some(Self::Contiguous { start, len });
        }
        if lanes.len() > 4 {
            return None;
        }
        let mut picked = [0u8; 4];
        for (slot, lane) in picked.iter_mut().zip(lanes) {
            *slot = u8::try_from(*lane).ok()?;
        }
        Some(Self::Picked {
            lanes: picked,
            len: lanes.len() as u8,
        })
    }

    fn len(&self) -> usize {
        match self {
            Self::Contiguous { len, .. } => usize::from(*len),
            Self::Picked { len, .. } => usize::from(*len),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.len()).map(move |i| match self {
            Self::Contiguous { start, .. } => usize::from(*start) + i,
            Self::Picked { lanes, .. } => usize::from(lanes[i]),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImportKey {
    Glsl { name: String, argc: usize },
    Lpfn { name: String, glsl_params: String },
    Vm { name: String, argc: usize },
    Texture { name: String, argc: usize },
}

#[derive(Debug, Clone)]
pub struct HirFunction {
    pub name: String,
    pub return_ty: LpsType,
    pub params: Vec<HirParam>,
    pub body: HirFunctionBody,
}

#[derive(Debug, Clone)]
pub struct HirParam {
    pub name: Option<String>,
    pub ty: LpsType,
    pub qualifier: ParamQualifier,
}

#[derive(Debug, Clone)]
pub struct HirFunctionBody {
    pub locals: Vec<HirLocal>,
    pub statements: Vec<HirStmt>,
    pub arena: HirArena,
}

#[derive(Debug, Clone)]
pub struct HirLocal {
    pub name: String,
    pub ty: LpsType,
}

#[derive(Debug, Clone)]
pub enum HirStmt {
    Let {
        local: usize,
        init: ExprId,
    },
    Assign {
        local: usize,
        value: ExprId,
    },
    If {
        condition: ExprId,
        accept: Vec<HirStmt>,
        reject: Vec<HirStmt>,
    },
    For {
        init: Vec<HirStmt>,
        condition: ExprId,
        continuing: Vec<HirStmt>,
        body: Vec<HirStmt>,
    },
    While {
        condition: ExprId,
        body: Vec<HirStmt>,
    },
    DoWhile {
        body: Vec<HirStmt>,
        condition: ExprId,
    },
    Break,
    Continue,
    Expr(ExprId),
    Return {
        expr: Option<ExprId>,
        span: Span,
    },
}

#[derive(Debug, Clone)]
pub struct HirExpr {
    pub span: Span,
    pub ty: LpsType,
    pub kind: HirExprKind,
}

#[derive(Debug, Clone)]
pub enum HirExprKind {
    BoolLiteral(bool),
    FloatLiteral(f32),
    IntLiteral(i32),
    UIntLiteral(u32),
    Param {
        index: usize,
    },
    Local {
        index: usize,
    },
    Uniform {
        byte_offset: u32,
    },
    Global {
        byte_offset: u32,
    },
    Constructor {
        args: ExprList,
    },
    Cast {
        expr: ExprId,
    },
    Swizzle {
        base: ExprId,
        lanes: SwizzleLanes,
    },
    Index {
        base: ExprId,
        index: ExprId,
    },
    Builtin {
        kind: BuiltinKind,
        args: ExprList,
        writebacks: WritebackList,
    },
    UserCall {
        function: usize,
        args: ExprList,
        writebacks: WritebackList,
    },
    ImportCall {
        import: ImportId,
        args: ExprList,
        out: Option<HirOutArg>,
    },
    TexelFetch {
        sampler: Box<HirTextureOperand>,
        coord: ExprId,
        lod: ExprId,
    },
    Texture {
        sampler: Box<HirTextureOperand>,
        coord: ExprId,
        import: ImportId,
    },
    Unary {
        op: UnaryOp,
        expr: ExprId,
    },
    Binary {
        op: BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
    },
    Sequence {
        first: ExprId,
        second: ExprId,
    },
    Conditional {
        condition: ExprId,
        accept: ExprId,
        reject: ExprId,
    },
    PlaceRead {
        target: PlaceId,
    },
    Assign {
        target: PlaceId,
        value: ExprId,
    },
    IncDec {
        target: PlaceId,
        op: IncDecOp,
        prefix: bool,
    },
}

/// An LPFN out-argument: the local it writes (whose declared type is the
/// out type — see [`HirFunctionBody::locals`]) and the position the
/// pointer takes in the call's argument list.
#[derive(Debug, Clone, Copy)]
pub struct HirOutArg {
    pub local: u32,
    pub arg_index: u32,
}

#[derive(Debug, Clone)]
pub struct HirTextureOperand {
    pub path: String,
    pub descriptor_byte_offset: u32,
}

/// One `out`/`inout` argument of a call: the place written back after the
/// call (its type is the place's, [`crate::hir::PlaceRecord::ty`]) and the
/// argument position it occupies. Lives in the arena's writeback list.
#[derive(Debug, Clone, Copy)]
pub struct HirUserCallWriteback {
    pub arg_index: u32,
    pub target: PlaceId,
    pub copy_in: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinKind {
    Abs,
    All,
    Any,
    BitCount,
    BitfieldExtract,
    BitfieldInsert,
    BitfieldReverse,
    Ceil,
    Clamp,
    Cross,
    Degrees,
    Determinant,
    Distance,
    Dot,
    Equal,
    Floor,
    Fma,
    Fract,
    FindLsb,
    FindMsb,
    GreaterThan,
    GreaterThanEqual,
    ImulExtended,
    Inverse,
    InverseSqrt,
    IsInf,
    IsNan,
    Length,
    LessThan,
    LessThanEqual,
    MatrixCompMult,
    Max,
    Min,
    Mix,
    Mod,
    Modf,
    Not,
    Normalize,
    NotEqual,
    OuterProduct,
    Radians,
    Round,
    RoundEven,
    Sign,
    Smoothstep,
    Sqrt,
    Step,
    Transpose,
    Trunc,
    UaddCarry,
    UmulExtended,
    UsubBorrow,
}
