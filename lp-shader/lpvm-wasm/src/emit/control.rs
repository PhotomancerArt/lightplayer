//! Structured control-flow stack for WASM emission.

use alloc::string::String;
use alloc::vec::Vec;

use wasm_encoder::InstructionSink;

/// WASM `block`/`if`/`loop` nesting depth (incremented when opening, decremented on `end`).
pub(crate) type WasmOpenDepth = u32;

#[derive(Clone, Debug)]
pub(crate) enum CtrlEntry {
    If,
    Else,
    /// `block` (break) → `loop` → `block` (body). `inner_closed` after `end` at `continuing_offset`.
    Loop {
        continuing_offset: u32,
        inner_closed: bool,
        /// [`WasmOpenDepth`] right after the outer `block` of the loop opened.
        outer_open_depth: WasmOpenDepth,
    },
    /// `switch` merge block; `merge_wasm_open` is depth after its `block` instruction.
    Switch {
        selector: u32,
        merge_wasm_open: WasmOpenDepth,
    },
    SwitchCaseArm,
    SwitchDefaultArm,
}

/// Before emitting the op at `pc`, close the inner `block` of any loop whose continuing section starts here.
pub(crate) fn close_loop_inner_at_continuing(
    sink: &mut InstructionSink<'_>,
    ctrl: &mut Vec<CtrlEntry>,
    wasm_open: &mut WasmOpenDepth,
    pc: usize,
) {
    for entry in ctrl.iter_mut().rev() {
        if let CtrlEntry::Loop {
            continuing_offset,
            inner_closed,
            ..
        } = entry
        {
            if !*inner_closed && *continuing_offset == pc as u32 {
                sink.end();
                *wasm_open = wasm_open.saturating_sub(1);
                *inner_closed = true;
                break;
            }
        }
    }
}

/// `br` depth to exit the innermost loop (and its outer break block).
pub(crate) fn innermost_loop_break_depth(
    ctrl: &[CtrlEntry],
    wasm_open: WasmOpenDepth,
) -> Result<u32, String> {
    for entry in ctrl.iter().rev() {
        if let CtrlEntry::Loop {
            outer_open_depth, ..
        } = entry
        {
            return Ok(wasm_open.saturating_sub(*outer_open_depth));
        }
    }
    Err(String::from("break/br_if_not outside loop"))
}

pub(crate) fn innermost_loop_continue_depth(
    ctrl: &[CtrlEntry],
    wasm_open: WasmOpenDepth,
) -> Result<u32, String> {
    for entry in ctrl.iter().rev() {
        if let CtrlEntry::Loop {
            inner_closed,
            outer_open_depth,
            ..
        } = entry
        {
            if *inner_closed {
                return Err(String::from("continue inside loop continuing section"));
            }
            // Inner body `block` sits at outer_open_depth + 2 (break block, loop, body block).
            // `br` depth = current nesting minus that target.
            return Ok(wasm_open.saturating_sub(*outer_open_depth + 2));
        }
    }
    Err(String::from("continue outside loop"))
}

pub(crate) fn innermost_switch_selector(ctrl: &[CtrlEntry]) -> Result<u32, String> {
    for entry in ctrl.iter().rev() {
        if let CtrlEntry::Switch { selector, .. } = entry {
            return Ok(*selector);
        }
    }
    Err(String::from("`case` / `default` outside `switch`"))
}

/// After `return`, emit closing instructions so the bytecode stays structurally balanced.
/// LPIR may omit `End` markers when a path returns from nested control.
///
/// Returning from a `switch` **case** closes only the case `if` (and inner `if`s), not the
/// merge `block`, so later `case` arms still emit correctly.
///
/// Returning from an `if` **then** branch must not emit `end` before the matching `Else` op:
/// LPIR still emits `else` / `end` for the same `if`, and WASM requires `else` to pair with the open `if`.
pub(crate) fn unwind_ctrl_after_return(
    _sink: &mut InstructionSink<'_>,
    _ctrl: &mut Vec<CtrlEntry>,
    _wasm_open: &mut WasmOpenDepth,
) {
    // Simplified: don't emit anything here.
    // The unreachable_mode flag will cause Op::End handlers to emit
    // the necessary ends and balance the stack.
}

pub(crate) fn switch_merge_open_depth(ctrl: &[CtrlEntry]) -> Result<WasmOpenDepth, String> {
    let mut seen_case = false;
    let mut i = ctrl.len();
    while i > 0 {
        i -= 1;
        match &ctrl[i] {
            CtrlEntry::SwitchCaseArm if !seen_case => {
                seen_case = true;
            }
            CtrlEntry::Switch {
                merge_wasm_open, ..
            } if seen_case => {
                return Ok(*merge_wasm_open);
            }
            _ => {}
        }
    }
    Err(String::from("internal: case arm without switch"))
}
