use alloc::string::String;
use alloc::vec::Vec;
use lp_collection::VecMap;

use lps_shared::{LpsModuleSig, LpsType, ParamQualifier, TextureBindingSpec};

use super::arena::{ExprId, ExprList, HirArena, PlaceId};
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
        lanes: Vec<usize>,
    },
    Index {
        base: ExprId,
        index: ExprId,
    },
    Builtin {
        kind: BuiltinKind,
        args: ExprList,
        writebacks: Vec<HirUserCallWriteback>,
    },
    UserCall {
        function: usize,
        args: ExprList,
        writebacks: Vec<HirUserCallWriteback>,
    },
    ImportCall {
        import: ImportKey,
        args: ExprList,
        out: Option<HirOutArg>,
    },
    TexelFetch {
        sampler: HirTextureOperand,
        coord: ExprId,
        lod: ExprId,
    },
    Texture {
        sampler: HirTextureOperand,
        coord: ExprId,
        import: ImportKey,
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

#[derive(Debug, Clone)]
pub struct HirOutArg {
    pub local: usize,
    pub ty: LpsType,
    pub arg_index: usize,
}

#[derive(Debug, Clone)]
pub struct HirTextureOperand {
    pub path: String,
    pub descriptor_byte_offset: u32,
}

#[derive(Debug, Clone)]
pub struct HirUserCallWriteback {
    pub arg_index: usize,
    pub target: PlaceId,
    pub ty: LpsType,
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
    Transpose,
    Trunc,
    UaddCarry,
    UmulExtended,
    UsubBorrow,
}
