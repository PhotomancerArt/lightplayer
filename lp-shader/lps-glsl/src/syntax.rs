use alloc::string::String;
use alloc::vec::Vec;

use crate::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFunctionBody {
    pub statements: Vec<ParsedStmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedStmt {
    Let {
        is_const: bool,
        ty: String,
        name: String,
        init: Option<ParsedExpr>,
        span: Span,
    },
    Assign {
        name: String,
        op: AssignOp,
        value: ParsedExpr,
        span: Span,
    },
    If {
        condition: ParsedExpr,
        accept: Vec<ParsedStmt>,
        reject: Vec<ParsedStmt>,
        span: Span,
    },
    For {
        init: Vec<ParsedStmt>,
        condition: Option<ParsedExpr>,
        continuing: Vec<ParsedStmt>,
        body: Vec<ParsedStmt>,
        span: Span,
    },
    While {
        condition: ParsedExpr,
        body: Vec<ParsedStmt>,
        span: Span,
    },
    DoWhile {
        body: Vec<ParsedStmt>,
        condition: ParsedExpr,
        span: Span,
    },
    Break {
        span: Span,
    },
    Continue {
        span: Span,
    },
    Block {
        statements: Vec<ParsedStmt>,
        span: Span,
    },
    Empty {
        span: Span,
    },
    Expr {
        expr: ParsedExpr,
        span: Span,
    },
    Return(ParsedExpr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedExpr {
    pub span: Span,
    pub kind: ParsedExprKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParsedExprKind {
    BoolLiteral(bool),
    FloatLiteral(f32),
    IntLiteral(i32),
    UIntLiteral(u32),
    Name(String),
    Call {
        name: String,
        args: Vec<ParsedExpr>,
    },
    Swizzle {
        base: alloc::boxed::Box<ParsedExpr>,
        fields: String,
    },
    Index {
        base: alloc::boxed::Box<ParsedExpr>,
        index: alloc::boxed::Box<ParsedExpr>,
    },
    Unary {
        op: UnaryOp,
        expr: alloc::boxed::Box<ParsedExpr>,
    },
    Binary {
        op: BinaryOp,
        lhs: alloc::boxed::Box<ParsedExpr>,
        rhs: alloc::boxed::Box<ParsedExpr>,
    },
    Conditional {
        condition: alloc::boxed::Box<ParsedExpr>,
        accept: alloc::boxed::Box<ParsedExpr>,
        reject: alloc::boxed::Box<ParsedExpr>,
    },
    Assign {
        target: alloc::boxed::Box<ParsedExpr>,
        value: alloc::boxed::Box<ParsedExpr>,
    },
    IncDec {
        target: alloc::boxed::Box<ParsedExpr>,
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
