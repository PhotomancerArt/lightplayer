use alloc::borrow::Cow;
use alloc::vec::Vec;

use crate::Span;

/// A parsed function body. Every name in it borrows the shader source
/// (`'src`): the parser never copies an identifier, and the body stage
/// allocates per node, not per name. The two composed spellings —
/// a declaration type with an array suffix after the name (`float x[3]`)
/// and an array constructor (`float[3](..)`) — are the only owned strings,
/// held in a [`Cow`] that stays borrowed everywhere else.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFunctionBody<'src> {
    pub statements: Vec<ParsedStmt<'src>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedStmt<'src> {
    Let {
        is_const: bool,
        ty: Cow<'src, str>,
        name: &'src str,
        init: Option<ParsedExpr<'src>>,
        span: Span,
    },
    LetGroup {
        is_const: bool,
        ty: Cow<'src, str>,
        declarations: Vec<ParsedLetDecl<'src>>,
        span: Span,
    },
    Assign {
        name: &'src str,
        op: AssignOp,
        value: ParsedExpr<'src>,
        span: Span,
    },
    If {
        condition: ParsedExpr<'src>,
        accept: Vec<ParsedStmt<'src>>,
        reject: Vec<ParsedStmt<'src>>,
        span: Span,
    },
    For {
        init: Vec<ParsedStmt<'src>>,
        condition: Option<ParsedExpr<'src>>,
        continuing: Vec<ParsedStmt<'src>>,
        body: Vec<ParsedStmt<'src>>,
        span: Span,
    },
    While {
        condition: ParsedExpr<'src>,
        body: Vec<ParsedStmt<'src>>,
        span: Span,
    },
    DoWhile {
        body: Vec<ParsedStmt<'src>>,
        condition: ParsedExpr<'src>,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    Block {
        statements: Vec<ParsedStmt<'src>>,
        span: Span,
    },
    Empty {
        span: Span,
    },
    Expr {
        expr: ParsedExpr<'src>,
        span: Span,
    },
    Return {
        expr: Option<ParsedExpr<'src>>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedLetDecl<'src> {
    pub ty: Cow<'src, str>,
    pub name: &'src str,
    pub init: Option<ParsedExpr<'src>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedExpr<'src> {
    pub span: Span,
    pub kind: ParsedExprKind<'src>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedExprKind<'src> {
    BoolLiteral(bool),
    FloatLiteral(f32),
    IntLiteral(i32),
    UIntLiteral(u32),
    Name(&'src str),
    Call {
        name: Cow<'src, str>,
        args: Vec<ParsedExpr<'src>>,
    },
    InitList {
        elements: Vec<ParsedExpr<'src>>,
    },
    Swizzle {
        base: alloc::boxed::Box<ParsedExpr<'src>>,
        fields: &'src str,
    },
    Length {
        base: alloc::boxed::Box<ParsedExpr<'src>>,
    },
    Index {
        base: alloc::boxed::Box<ParsedExpr<'src>>,
        index: alloc::boxed::Box<ParsedExpr<'src>>,
    },
    Unary {
        op: UnaryOp,
        expr: alloc::boxed::Box<ParsedExpr<'src>>,
    },
    Binary {
        op: BinaryOp,
        lhs: alloc::boxed::Box<ParsedExpr<'src>>,
        rhs: alloc::boxed::Box<ParsedExpr<'src>>,
    },
    Conditional {
        condition: alloc::boxed::Box<ParsedExpr<'src>>,
        accept: alloc::boxed::Box<ParsedExpr<'src>>,
        reject: alloc::boxed::Box<ParsedExpr<'src>>,
    },
    Assign {
        target: alloc::boxed::Box<ParsedExpr<'src>>,
        op: AssignOp,
        value: alloc::boxed::Box<ParsedExpr<'src>>,
    },
    IncDec {
        target: alloc::boxed::Box<ParsedExpr<'src>>,
        op: IncDecOp,
        prefix: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Comma,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    LogicalAnd,
    LogicalOr,
    LogicalXor,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Set,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncDecOp {
    Increment,
    Decrement,
}
