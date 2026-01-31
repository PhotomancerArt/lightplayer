use glsl::syntax::{SimpleStatement, Statement};

use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::context::CodegenContext;
use alloc::format;

// Submodules for each statement type
pub mod r#break;
pub mod compound;
pub mod r#continue;
pub mod declaration;
pub mod expr;
pub mod r#if;
pub mod jump;
pub mod loop_do_while;
pub mod loop_for;
pub mod loop_while;
pub mod loops;
pub mod r#return;

impl<'a, M: cranelift_module::Module> CodegenContext<'a, M> {
    /// Main statement dispatch function (renamed from translate_statement to match Clang's EmitStmt)
    pub fn emit_statement(&mut self, stmt: &Statement) -> Result<(), GlslError> {
        match stmt {
            Statement::Simple(simple) => self.emit_simple_statement(simple),
            Statement::Compound(compound) => self.emit_compound_stmt(compound),
        }
    }

    /// Dispatch for simple statements
    fn emit_simple_statement(&mut self, stmt: &SimpleStatement) -> Result<(), GlslError> {
        match stmt {
            SimpleStatement::Declaration(decl) => self.emit_declaration(decl),
            SimpleStatement::Expression(Some(expr)) => self.emit_expr_stmt(expr),
            SimpleStatement::Expression(None) => Ok(()), // Empty statement
            SimpleStatement::Selection(selection) => self.emit_if_stmt(selection),
            SimpleStatement::Iteration(iteration) => self.emit_iteration_stmt(iteration),
            SimpleStatement::Jump(jump) => self.emit_jump_stmt(jump),
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                format!("statement type not supported: {stmt:?}"),
            )),
        }
    }

    // Delegate to submodules
    pub fn emit_compound_stmt(
        &mut self,
        compound: &glsl::syntax::CompoundStatement,
    ) -> Result<(), GlslError> {
        compound::emit_compound_stmt(self, compound)
    }

    pub fn emit_if_stmt(
        &mut self,
        selection: &glsl::syntax::SelectionStatement,
    ) -> Result<(), GlslError> {
        r#if::emit_if_stmt(self, selection)
    }

    pub fn emit_iteration_stmt(
        &mut self,
        iteration: &glsl::syntax::IterationStatement,
    ) -> Result<(), GlslError> {
        loops::emit_iteration_stmt(self, iteration)
    }

    pub fn emit_jump_stmt(&mut self, jump: &glsl::syntax::JumpStatement) -> Result<(), GlslError> {
        jump::emit_jump_stmt(self, jump)
    }

    pub fn emit_declaration(&mut self, decl: &glsl::syntax::Declaration) -> Result<(), GlslError> {
        declaration::emit_declaration(self, decl)
    }

    pub fn emit_expr_stmt(&mut self, expr: &glsl::syntax::Expr) -> Result<(), GlslError> {
        expr::emit_expr_stmt(self, expr)
    }
}
