//! Replace a [`LpirOp::Call`] with an inlined, remapped callee body.

use alloc::vec::Vec;

use crate::inline::remap::{build_remap, remap_op, scan_param_writes};
use crate::lpir_module::IrFunction;
use crate::lpir_op::LpirOp;
use crate::types::VReg;

enum ReturnShape {
    None,
    SingleAtEnd,
    Multi,
}

fn classify_return_shape(body: &[LpirOp]) -> ReturnShape {
    let mut return_indices = Vec::new();
    for (i, op) in body.iter().enumerate() {
        if matches!(op, LpirOp::Return { .. }) {
            return_indices.push(i);
        }
    }
    match return_indices.len() {
        0 => ReturnShape::None,
        1 => {
            let ri = return_indices[0];
            if ri + 1 == body.len() {
                ReturnShape::SingleAtEnd
            } else {
                ReturnShape::Multi
            }
        }
        _ => ReturnShape::Multi,
    }
}

pub(crate) fn inline_call_site(caller: &mut IrFunction, callee: &IrFunction, call_op_idx: usize) {
    let (args_range, results_range) = match &caller.body.get(call_op_idx) {
        Some(LpirOp::Call { args, results, .. }) => (*args, *results),
        _ => return,
    };

    let call_args: Vec<VReg> = caller.pool_slice(args_range).to_vec();
    let call_results: Vec<VReg> = caller.pool_slice(results_range).to_vec();

    debug_assert_eq!(
        call_args.len(),
        1 + callee.param_count as usize,
        "inline call args arity"
    );
    debug_assert_eq!(
        call_results.len(),
        callee.return_types.len(),
        "inline call results arity"
    );
    if call_args.len() != 1 + callee.param_count as usize
        || call_results.len() != callee.return_types.len()
    {
        return;
    }

    let pw = scan_param_writes(callee);
    let rmap = build_remap(caller, callee, &call_args, &call_results, &pw);

    let shape = classify_return_shape(&callee.body);
    let needs_block = matches!(shape, ReturnShape::Multi);

    let mut scratch: Vec<LpirOp> = Vec::new();
    scratch.extend_from_slice(&rmap.param_copies);

    if needs_block {
        scratch.push(LpirOp::Block { end_offset: 0 });
    }

    let mut last_was_exit_block = false;

    for op in &callee.body {
        match op {
            LpirOp::Return { values } => {
                let vals = callee.pool_slice(*values);
                if vals.len() != call_results.len() {
                    return;
                }
                debug_assert_eq!(vals.len(), call_results.len());
                for (k, &src_raw) in vals.iter().enumerate() {
                    let src = rmap.vreg_table[src_raw.0 as usize];
                    scratch.push(LpirOp::Copy {
                        dst: call_results[k],
                        src,
                    });
                }
                if needs_block {
                    scratch.push(LpirOp::ExitBlock);
                    last_was_exit_block = true;
                } else {
                    last_was_exit_block = false;
                }
            }
            _ => {
                last_was_exit_block = false;
                scratch.push(remap_op(
                    op,
                    &rmap,
                    &mut caller.vreg_pool,
                    &callee.vreg_pool,
                ));
            }
        }
    }

    if needs_block && !last_was_exit_block {
        scratch.push(LpirOp::ExitBlock);
    }

    if needs_block {
        scratch.push(LpirOp::End);
    }

    caller.body.splice(call_op_idx..=call_op_idx, scratch);
}
