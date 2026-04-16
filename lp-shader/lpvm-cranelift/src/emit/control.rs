use alloc::vec::Vec;

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{Block, InstBuilder};
use cranelift_frontend::{FunctionBuilder, Variable};
use lpir::lpir_module::IrFunction;
use lpir::lpir_op::LpirOp;

use super::{CtrlFrame, switch_to_unreachable_tail, use_v};
use crate::error::CompileError;

/// If the current block exists, is not `target`, and has no terminator yet, insert a `jump` to
/// `target`. The `cur == target` guard prevents accidental self-loops (e.g. Switch End when
/// Default End already placed us on `merge_block`).
fn jump_if_unterminated(builder: &mut FunctionBuilder, target: Block) {
    if let Some(cur) = builder.current_block() {
        if cur == target {
            return;
        }
        let needs = match builder.func.layout.last_inst(cur) {
            None => true,
            Some(last) => !builder.func.dfg.insts[last].opcode().is_terminator(),
        };
        if needs {
            builder.ins().jump(target, &[]);
        }
    }
}

pub(crate) fn maybe_enter_loop_continue_region(
    builder: &mut FunctionBuilder,
    ctrl_stack: &[CtrlFrame],
    op_idx: usize,
) -> Result<(), CompileError> {
    for frame in ctrl_stack.iter().rev() {
        if let CtrlFrame::Loop {
            header_block,
            continue_block,
            loop_start_pc,
            continue_pc,
            ..
        } = frame
        {
            let _ = header_block;
            if op_idx == *continue_pc
                && *continue_pc > *loop_start_pc + 1
                && *header_block != *continue_block
            {
                jump_if_unterminated(builder, *continue_block);
                builder.switch_to_block(*continue_block);
                break;
            }
        }
    }
    Ok(())
}

pub(crate) fn emit_control(
    op: &LpirOp,
    func: &IrFunction,
    builder: &mut FunctionBuilder,
    vars: &[Variable],
    ctrl_stack: &mut Vec<CtrlFrame>,
    op_idx: usize,
) -> Result<bool, CompileError> {
    let _ = func;
    match op {
        LpirOp::IfStart { cond, .. } => {
            let cond_val = use_v(builder, vars, *cond);
            let then_block = builder.create_block();
            let else_block = builder.create_block();
            let merge_block = builder.create_block();
            let pred = builder.ins().icmp_imm(IntCC::NotEqual, cond_val, 0);
            builder.ins().brif(pred, then_block, &[], else_block, &[]);
            builder.switch_to_block(then_block);
            ctrl_stack.push(CtrlFrame::If {
                else_block,
                merge_block,
            });
            Ok(true)
        }
        LpirOp::Else => match ctrl_stack.pop() {
            Some(CtrlFrame::If {
                else_block,
                merge_block,
            }) => {
                jump_if_unterminated(builder, merge_block);
                builder.switch_to_block(else_block);
                ctrl_stack.push(CtrlFrame::Else { merge_block });
                Ok(true)
            }
            _ => Err(CompileError::unsupported("else without matching if")),
        },
        LpirOp::LoopStart {
            continuing_offset, ..
        } => {
            let loop_start_pc = op_idx;
            let continue_pc = *continuing_offset as usize;
            let exit_block = builder.create_block();
            let header_block = builder.create_block();
            let continue_block = if continue_pc > loop_start_pc + 1 {
                builder.create_block()
            } else {
                header_block
            };
            builder.ins().jump(header_block, &[]);
            builder.switch_to_block(header_block);
            ctrl_stack.push(CtrlFrame::Loop {
                header_block,
                continue_block,
                exit_block,
                loop_start_pc,
                continue_pc,
            });
            Ok(true)
        }
        LpirOp::Break => {
            let exit = find_innermost_loop_exit(ctrl_stack)?;
            builder.ins().jump(exit, &[]);
            switch_to_unreachable_tail(builder);
            Ok(true)
        }
        LpirOp::Continue => {
            let cont = find_innermost_loop_continue(ctrl_stack)?;
            builder.ins().jump(cont, &[]);
            switch_to_unreachable_tail(builder);
            Ok(true)
        }
        LpirOp::BrIfNot { cond } => {
            let cond_val = use_v(builder, vars, *cond);
            let exit = find_innermost_loop_exit(ctrl_stack)?;
            let continue_block = builder.create_block();
            let pred = builder.ins().icmp_imm(IntCC::NotEqual, cond_val, 0);
            builder.ins().brif(pred, continue_block, &[], exit, &[]);
            builder.switch_to_block(continue_block);
            Ok(true)
        }
        LpirOp::SwitchStart { selector, .. } => {
            let selector_val = use_v(builder, vars, *selector);
            let merge_block = builder.create_block();
            let first_case_block = builder.create_block();
            builder.ins().jump(first_case_block, &[]);
            builder.switch_to_block(first_case_block);
            ctrl_stack.push(CtrlFrame::Switch {
                selector: selector_val,
                merge_block,
            });
            Ok(true)
        }
        LpirOp::CaseStart { value, .. } => {
            let (selector, merge_block) = find_innermost_switch(ctrl_stack)?;
            let body_block = builder.create_block();
            let next_case_block = builder.create_block();
            let cmp = builder
                .ins()
                .icmp_imm(IntCC::Equal, selector, i64::from(*value));
            builder
                .ins()
                .brif(cmp, body_block, &[], next_case_block, &[]);
            builder.switch_to_block(body_block);
            ctrl_stack.push(CtrlFrame::Case {
                merge_block,
                next_case_block,
            });
            Ok(true)
        }
        LpirOp::DefaultStart { .. } => {
            let (_, merge_block) = find_innermost_switch(ctrl_stack)?;
            if builder.current_block().is_none() {
                return Err(CompileError::unsupported("default outside block"));
            }
            ctrl_stack.push(CtrlFrame::Default { merge_block });
            Ok(true)
        }
        LpirOp::End => match ctrl_stack.pop() {
            Some(CtrlFrame::If {
                else_block,
                merge_block,
            }) => {
                jump_if_unterminated(builder, merge_block);
                builder.switch_to_block(else_block);
                builder.ins().jump(merge_block, &[]);
                builder.switch_to_block(merge_block);
                Ok(true)
            }
            Some(CtrlFrame::Else { merge_block }) => {
                jump_if_unterminated(builder, merge_block);
                builder.switch_to_block(merge_block);
                Ok(true)
            }
            Some(CtrlFrame::Loop {
                header_block,
                exit_block,
                ..
            }) => {
                jump_if_unterminated(builder, header_block);
                builder.switch_to_block(exit_block);
                Ok(true)
            }
            Some(CtrlFrame::Case {
                merge_block,
                next_case_block,
            }) => {
                jump_if_unterminated(builder, merge_block);
                builder.switch_to_block(next_case_block);
                Ok(true)
            }
            Some(CtrlFrame::Default { merge_block }) => {
                jump_if_unterminated(builder, merge_block);
                builder.switch_to_block(merge_block);
                Ok(true)
            }
            Some(CtrlFrame::Switch { merge_block, .. }) => {
                jump_if_unterminated(builder, merge_block);
                builder.switch_to_block(merge_block);
                Ok(true)
            }
            None => Err(CompileError::unsupported("`end` with empty control stack")),
        },
        _ => Ok(false),
    }
}

fn find_innermost_loop_exit(ctrl_stack: &[CtrlFrame]) -> Result<Block, CompileError> {
    for frame in ctrl_stack.iter().rev() {
        if let CtrlFrame::Loop { exit_block, .. } = frame {
            return Ok(*exit_block);
        }
    }
    Err(CompileError::unsupported("break/br_if_not outside loop"))
}

fn find_innermost_loop_continue(ctrl_stack: &[CtrlFrame]) -> Result<Block, CompileError> {
    for frame in ctrl_stack.iter().rev() {
        if let CtrlFrame::Loop { continue_block, .. } = frame {
            return Ok(*continue_block);
        }
    }
    Err(CompileError::unsupported("continue outside loop"))
}

fn find_innermost_switch(
    ctrl_stack: &[CtrlFrame],
) -> Result<(cranelift_codegen::ir::Value, Block), CompileError> {
    for frame in ctrl_stack.iter().rev() {
        if let CtrlFrame::Switch {
            selector,
            merge_block,
        } = frame
        {
            return Ok((*selector, *merge_block));
        }
    }
    Err(CompileError::unsupported("case/default outside switch"))
}
