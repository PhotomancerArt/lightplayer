use glsl::syntax::JumpStatement;

use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::context::CodegenContext;

use alloc::format;

/// Emit jump statement (dispatch to break, continue, return)
pub fn emit_jump_stmt<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    jump: &JumpStatement,
) -> Result<(), GlslError> {
    use glsl::syntax::JumpStatement;

    match jump {
        JumpStatement::Break => crate::frontend::codegen::stmt::r#break::emit_break_stmt(ctx),
        JumpStatement::Continue => {
            crate::frontend::codegen::stmt::r#continue::emit_continue_stmt(ctx)
        }
        JumpStatement::Return(expr) => crate::frontend::codegen::stmt::r#return::emit_return_stmt(
            ctx,
            expr.as_ref().map(|v| &**v),
        ),
        _ => Err(GlslError::new(
            ErrorCode::E0400,
            format!("jump statement not supported: {jump:?}"),
        )),
    }
}
