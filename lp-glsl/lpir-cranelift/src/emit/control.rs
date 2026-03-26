use alloc::vec::Vec;

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{Block, InstBuilder};
use cranelift_frontend::{FunctionBuilder, Variable};
use lpir::module::IrFunction;
use lpir::op::Op;

use super::{CtrlFrame, switch_to_unreachable_tail, use_v};
use crate::error::CompileError;

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
            if op_idx == *continue_pc
                && *continue_pc > *loop_start_pc + 1
                && *header_block != *continue_block
            {
                if builder.current_block() == Some(*header_block) {
                    builder.ins().jump(*continue_block, &[]);
                    builder.switch_to_block(*continue_block);
                }
                break;
            }
        }
    }
    Ok(())
}

pub(crate) fn emit_control(
    op: &Op,
    func: &IrFunction,
    builder: &mut FunctionBuilder,
    vars: &[Variable],
    ctrl_stack: &mut Vec<CtrlFrame>,
    op_idx: usize,
) -> Result<bool, CompileError> {
    let _ = func;
    match op {
        Op::IfStart { cond, .. } => {
            let cond_val = use_v(builder, vars, *cond);
            let then_block = builder.create_block();
            let else_block = builder.create_block();
            let merge_block = builder.create_block();
            let pred = builder.ins().icmp_imm(IntCC::NotEqual, cond_val, 0);
            builder.ins().brif(pred, then_block, &[], else_block, &[]);
            builder.switch_to_block(then_block);
            ctrl_stack.push(CtrlFrame::If {
                then_block,
                else_block,
                merge_block,
            });
            Ok(true)
        }
        Op::Else => match ctrl_stack.pop() {
            Some(CtrlFrame::If {
                then_block,
                else_block,
                merge_block,
            }) => {
                if builder.current_block() == Some(then_block) {
                    builder.ins().jump(merge_block, &[]);
                }
                builder.switch_to_block(else_block);
                ctrl_stack.push(CtrlFrame::Else {
                    else_block,
                    merge_block,
                });
                Ok(true)
            }
            _ => Err(CompileError::unsupported("else without matching if")),
        },
        Op::LoopStart {
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
        Op::Break => {
            let exit = find_innermost_loop_exit(ctrl_stack)?;
            builder.ins().jump(exit, &[]);
            switch_to_unreachable_tail(builder);
            Ok(true)
        }
        Op::Continue => {
            let cont = find_innermost_loop_continue(ctrl_stack)?;
            builder.ins().jump(cont, &[]);
            switch_to_unreachable_tail(builder);
            Ok(true)
        }
        Op::BrIfNot { cond } => {
            let cond_val = use_v(builder, vars, *cond);
            let exit = find_innermost_loop_exit(ctrl_stack)?;
            let continue_block = builder.create_block();
            let pred = builder.ins().icmp_imm(IntCC::NotEqual, cond_val, 0);
            builder.ins().brif(pred, continue_block, &[], exit, &[]);
            builder.switch_to_block(continue_block);
            Ok(true)
        }
        Op::SwitchStart { selector, .. } => {
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
        Op::CaseStart { value, .. } => {
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
                body_block,
                merge_block,
                next_case_block,
            });
            Ok(true)
        }
        Op::DefaultStart { .. } => {
            let (_, merge_block) = find_innermost_switch(ctrl_stack)?;
            let entry_block = builder
                .current_block()
                .ok_or_else(|| CompileError::unsupported("default outside block"))?;
            ctrl_stack.push(CtrlFrame::Default {
                entry_block,
                merge_block,
            });
            Ok(true)
        }
        Op::End => match ctrl_stack.pop() {
            Some(CtrlFrame::If {
                then_block,
                else_block,
                merge_block,
            }) => {
                if builder.current_block() == Some(then_block) {
                    builder.ins().jump(merge_block, &[]);
                }
                builder.switch_to_block(else_block);
                builder.ins().jump(merge_block, &[]);
                builder.switch_to_block(merge_block);
                Ok(true)
            }
            Some(CtrlFrame::Else {
                else_block,
                merge_block,
            }) => {
                if builder.current_block() == Some(else_block) {
                    builder.ins().jump(merge_block, &[]);
                }
                builder.switch_to_block(merge_block);
                Ok(true)
            }
            Some(CtrlFrame::Loop {
                header_block,
                exit_block,
                ..
            }) => {
                if let Some(cur) = builder.current_block() {
                    let needs_back = match builder.func.layout.last_inst(cur) {
                        None => true,
                        Some(last) => !builder.func.dfg.insts[last].opcode().is_terminator(),
                    };
                    if needs_back {
                        builder.ins().jump(header_block, &[]);
                    }
                }
                builder.switch_to_block(exit_block);
                Ok(true)
            }
            Some(CtrlFrame::Case {
                body_block,
                merge_block,
                next_case_block,
            }) => {
                if builder.current_block() == Some(body_block) {
                    builder.ins().jump(merge_block, &[]);
                }
                builder.switch_to_block(next_case_block);
                Ok(true)
            }
            Some(CtrlFrame::Default {
                entry_block,
                merge_block,
            }) => {
                if builder.current_block() == Some(entry_block) {
                    builder.ins().jump(merge_block, &[]);
                }
                builder.switch_to_block(merge_block);
                Ok(true)
            }
            Some(CtrlFrame::Switch { merge_block, .. }) => {
                if builder.current_block() != Some(merge_block) {
                    builder.ins().jump(merge_block, &[]);
                }
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
