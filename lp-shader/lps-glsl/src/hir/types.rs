use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use lps_shared::{LpsModuleSig, LpsType, ParamQualifier};

use super::place::HirPlace;
use crate::Span;
use crate::body::{BinaryOp, IncDecOp, UnaryOp};

#[derive(Debug, Clone)]
pub struct HirModule {
    pub functions: Vec<HirFunction>,
    pub meta: LpsModuleSig,
    pub uniforms: BTreeMap<String, UniformInfo>,
    pub globals: BTreeMap<String, GlobalInfo>,
    pub imports: Vec<ImportInfo>,
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
}

pub(super) type StructTypes = BTreeMap<String, LpsType>;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImportKey {
    Glsl { name: String, argc: usize },
    Lpfn { name: String, glsl_params: String },
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
        init: HirExpr,
    },
    Assign {
        local: usize,
        value: HirExpr,
    },
    If {
        condition: HirExpr,
        accept: Vec<HirStmt>,
        reject: Vec<HirStmt>,
    },
    For {
        init: Vec<HirStmt>,
        condition: HirExpr,
        continuing: Vec<HirStmt>,
        body: Vec<HirStmt>,
    },
    While {
        condition: HirExpr,
        body: Vec<HirStmt>,
    },
    DoWhile {
        body: Vec<HirStmt>,
        condition: HirExpr,
    },
    Break,
    Continue,
    Expr(HirExpr),
    Return {
        expr: Option<HirExpr>,
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
        name: String,
        byte_offset: u32,
    },
    Global {
        name: String,
        byte_offset: u32,
    },
    Constructor {
        args: Vec<HirExpr>,
    },
    Cast {
        expr: Box<HirExpr>,
    },
    Swizzle {
        base: Box<HirExpr>,
        lanes: Vec<usize>,
    },
    Index {
        base: Box<HirExpr>,
        index: Box<HirExpr>,
    },
    Builtin {
        kind: BuiltinKind,
        args: Vec<HirExpr>,
    },
    UserCall {
        function: usize,
        args: Vec<HirExpr>,
        writebacks: Vec<HirUserCallWriteback>,
    },
    ImportCall {
        import: ImportKey,
        args: Vec<HirExpr>,
        out: Option<HirOutArg>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<HirExpr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<HirExpr>,
        rhs: Box<HirExpr>,
    },
    Sequence {
        first: Box<HirExpr>,
        second: Box<HirExpr>,
    },
    Conditional {
        condition: Box<HirExpr>,
        accept: Box<HirExpr>,
        reject: Box<HirExpr>,
    },
    PlaceRead {
        target: HirAssignTarget,
    },
    Assign {
        target: HirAssignTarget,
        value: Box<HirExpr>,
    },
    IncDec {
        target: HirAssignTarget,
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
pub struct HirUserCallWriteback {
    pub arg_index: usize,
    pub target: HirAssignTarget,
    pub ty: LpsType,
    pub copy_in: bool,
}

#[derive(Debug, Clone)]
pub struct HirAssignTarget {
    pub(crate) place: HirPlace,
}

impl HirAssignTarget {
    pub(super) fn ty(&self) -> &LpsType {
        &self.place.ty
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinKind {
    Abs,
    All,
    Any,
    BitCount,
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
}
