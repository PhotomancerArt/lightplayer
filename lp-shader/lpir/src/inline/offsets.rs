//! Structural control-flow offset recompute for flat [`LpirOp`] bodies.

use alloc::vec::Vec;

use crate::lpir_op::LpirOp;

enum Frame {
    If {
        start: usize,
    },
    Else {
        if_start: usize,
    },
    Loop {
        start: usize,
        had_continuing: bool,
    },
    Block {
        start: usize,
    },
    Switch {
        start: usize,
        /// Index of `CaseStart` / `DefaultStart` whose `end_offset` points to the next arm opener
        /// or the switch's closing `End`.
        pending_case: Option<usize>,
    },
    /// Inside a `case` / `default` arm (closed by one `End` per arm).
    Arm,
}

/// Recompute all control-flow offset fields in `body`. Idempotent; overwrites existing offsets.
pub(crate) fn recompute_offsets(body: &mut [LpirOp]) {
    let mut stack: Vec<Frame> = Vec::new();

    for idx in 0..body.len() {
        let after = (idx + 1) as u32;

        match &mut body[idx] {
            LpirOp::IfStart {
                else_offset,
                end_offset,
                ..
            } => {
                *else_offset = 0;
                *end_offset = 0;
                stack.push(Frame::If { start: idx });
            }
            LpirOp::Else => {
                let top = stack
                    .pop()
                    .expect("Else without matching IfStart");
                match top {
                    Frame::If { start } => {
                        if let LpirOp::IfStart {
                            else_offset,
                            end_offset: _,
                            ..
                        } = &mut body[start]
                        {
                            *else_offset = idx as u32;
                        } else {
                            panic!("Else: expected IfStart at {start}");
                        }
                        stack.push(Frame::Else { if_start: start });
                    }
                    _ => panic!("Else: expected If frame"),
                }
            }
            LpirOp::Continuing => {
                let top = stack.last_mut().expect("Continuing outside loop");
                match top {
                    Frame::Loop {
                        start,
                        had_continuing,
                    } => {
                        assert!(
                            !*had_continuing,
                            "duplicate Continuing in same loop"
                        );
                        *had_continuing = true;
                        if let LpirOp::LoopStart {
                            continuing_offset, ..
                        } = &mut body[*start]
                        {
                            *continuing_offset = idx as u32;
                        } else {
                            panic!("Continuing: expected LoopStart");
                        }
                    }
                    _ => panic!("Continuing: expected Loop frame"),
                }
            }
            LpirOp::LoopStart {
                continuing_offset,
                end_offset,
            } => {
                *continuing_offset = 0;
                *end_offset = 0;
                stack.push(Frame::Loop {
                    start: idx,
                    had_continuing: false,
                });
            }
            LpirOp::SwitchStart { end_offset, .. } => {
                *end_offset = 0;
                stack.push(Frame::Switch {
                    start: idx,
                    pending_case: None,
                });
            }
            LpirOp::CaseStart { end_offset, .. } | LpirOp::DefaultStart { end_offset } => {
                *end_offset = 0;
                let pending = if let Some(Frame::Switch {
                    pending_case, ..
                }) = stack.last_mut()
                {
                    pending_case.take()
                } else {
                    panic!("Case/Default outside Switch");
                };
                if let Some(pc) = pending {
                    match &mut body[pc] {
                        LpirOp::CaseStart { end_offset: eo, .. }
                        | LpirOp::DefaultStart { end_offset: eo } => {
                            *eo = idx as u32;
                        }
                        _ => {}
                    }
                }
                if let Some(Frame::Switch {
                    pending_case, ..
                }) = stack.last_mut()
                {
                    *pending_case = Some(idx);
                }
                stack.push(Frame::Arm);
            }
            LpirOp::Block { end_offset } => {
                *end_offset = 0;
                stack.push(Frame::Block { start: idx });
            }
            LpirOp::ExitBlock => {}
            LpirOp::End => {
                let end_idx = idx;
                let frame = stack.pop().expect("End without matching opener");
                match frame {
                    Frame::Arm => {}
                    Frame::Else { if_start } => {
                        if let LpirOp::IfStart { end_offset, .. } = &mut body[if_start] {
                            *end_offset = after;
                        } else {
                            panic!("End: expected IfStart");
                        }
                    }
                    Frame::If { start } => {
                        if let LpirOp::IfStart {
                            else_offset,
                            end_offset,
                            ..
                        } = &mut body[start]
                        {
                            *else_offset = end_idx as u32;
                            *end_offset = after;
                        } else {
                            panic!("End: expected IfStart");
                        }
                    }
                    Frame::Loop {
                        start,
                        had_continuing,
                    } => {
                        if let LpirOp::LoopStart {
                            continuing_offset,
                            end_offset,
                        } = &mut body[start]
                        {
                            if !had_continuing {
                                *continuing_offset = (start + 1) as u32;
                            }
                            *end_offset = after;
                        } else {
                            panic!("End: expected LoopStart");
                        }
                    }
                    Frame::Block { start } => {
                        if let LpirOp::Block { end_offset } = &mut body[start] {
                            *end_offset = after;
                        } else {
                            panic!("End: expected Block");
                        }
                    }
                    Frame::Switch {
                        start,
                        pending_case,
                    } => {
                        if let Some(pc) = pending_case {
                            match &mut body[pc] {
                                LpirOp::CaseStart { end_offset: eo, .. }
                                | LpirOp::DefaultStart { end_offset: eo } => {
                                    *eo = end_idx as u32;
                                }
                                _ => {}
                            }
                        }
                        if let LpirOp::SwitchStart { end_offset, .. } = &mut body[start] {
                            *end_offset = after;
                        } else {
                            panic!("End: expected SwitchStart");
                        }
                    }
                }
            }
            _ => {}
        }
    }

    debug_assert!(
        stack.is_empty(),
        "recompute_offsets: unclosed frames: {:?}",
        stack.len()
    );
}
