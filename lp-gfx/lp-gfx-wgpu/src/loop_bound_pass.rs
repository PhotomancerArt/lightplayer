//! Loop bounds for the GPU tier: a static refusal of loops with no exit, and
//! an injected per-invocation back-edge budget mirroring the LPVM fuel meter.
//!
//! Every LPVM tier meters fuel — [`lp_shader::DEFAULT_INVOCATION_FUEL`] loop
//! back-edges per invocation — and traps when the tank is empty. A GPU has no
//! meter: a kernel that never returns is stopped only by the driver's
//! watchdog, which resets the device and takes every other surface in flight
//! with it (`docs/defects/2026-09-06-gpu-tier-executes-unbounded-shaders.md`).
//! This module closes that contract gap in two layers, both run between
//! `glsl-in` and validation:
//!
//! 1. [`refuse_loops_without_exit`] — a loop whose body has no `break`,
//!    `return` or `discard` on any path is a compile error naming the
//!    function and the authored line (with the same `┌─ glsl:LINE:COL`
//!    marker naga's diagnostics carry, in the same authored coordinates —
//!    see [`crate::assembly::AssembledGlsl`]). Constant `if` conditions are
//!    folded first, because glsl-in lowers `while (true)` to a loop whose
//!    body opens with `if (!true) { break; }` — the demo's exit is
//!    syntactically present and semantically dead. The check is
//!    conservative the safe way round: it only refuses loops with *no*
//!    exit, and only in functions an entry point can reach (a loop nothing
//!    calls cannot run, and the LPVM tiers compile such a unit); a
//!    data-dependent runaway compiles and is left to layer 2.
//! 2. [`bound_loop_iterations`] — every remaining loop charges one unit of a
//!    per-invocation `var<private>` budget in its `continuing` block and
//!    `break if`s once the budget is spent. The unit is the loop back-edge,
//!    exactly the fuel meter's, and the budget is the same constant. Unlike
//!    the LPVM, the GPU cannot trap mid-invocation: an exhausted budget
//!    exits the loop and the invocation completes with whatever it has. So
//!    the pass also gives the module one `@group(0)` storage global
//!    `atomic<u32>` — the **fault flag** — and the invocation that crosses
//!    the budget adds one to it, exactly once (the budget keeps counting
//!    past the cap, so only the first loop to cross sees `spent == cap +
//!    1`; every later loop in that invocation breaks without charging). The
//!    host clears the flag before each dispatch and reads it after
//!    ([`crate::fault_flag`]): a non-zero count is the GPU tier's fuel
//!    trap, reported as `GfxError::FuelExhausted` with the count of
//!    invocations that ran dry.
//!
//! Neither layer rebuilds an expression arena: the budget's expressions are
//! appended (operands precede uses by construction) and only `continuing`
//! blocks and `break_if` slots are touched, so existing `Emit` ranges stay
//! valid.

use naga::{
    AddressSpace, Arena, AtomicFunction, BinaryOperator, Block, Expression, Function,
    GlobalVariable, Handle, Literal, Module, Range, ResourceBinding, Scalar, Span, Statement,
    StorageAccess, Type, TypeInner, UnaryOperator,
};

use crate::assembly::AssembledGlsl;

/// Name of the injected per-invocation budget global (`var<private>`).
pub const LOOP_BUDGET_GLOBAL: &str = "lp_gfx_loop_budget";

/// Name of the injected fault flag (`@group(0) var<storage, read_write>
/// …: atomic<u32>`): the number of invocations of the dispatch that spent
/// their budget.
pub const LOOP_FAULT_GLOBAL: &str = "lp_gfx_loop_fault";

/// What [`bound_loop_iterations`] did to a module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LoopBounds {
    /// Loops charged.
    pub loops: usize,
    /// `@group(0)` binding of the fault flag; `None` for a loop-free module,
    /// which gets no flag.
    pub fault_binding: Option<u32>,
}

/// Refuse any loop whose body has no exit on any path.
///
/// `unit` is the assembled GLSL the module was parsed from, with its
/// authored range; the refusal names the offending loop's authored line.
/// Call after `glsl-in`, before validation.
pub fn refuse_loops_without_exit(module: &Module, unit: &AssembledGlsl) -> Result<(), String> {
    for handle in reachable_functions(module) {
        let function = &module.functions[handle];
        check_block(module, function, &function.body, unit)?;
    }
    for entry_point in &module.entry_points {
        check_block(
            module,
            &entry_point.function,
            &entry_point.function.body,
            unit,
        )?;
    }
    Ok(())
}

/// Charge every loop one unit of a per-invocation budget per back-edge,
/// break out once `budget` units are spent, and count the invocation that
/// crossed the budget on the module's fault flag.
///
/// Returns how many loops were bounded and where the fault flag is bound.
/// A module without loops is left untouched (no globals, no expressions).
/// Call after [`refuse_loops_without_exit`] and after every other
/// `@group(0)` binding has been assigned (the flag takes the next free
/// slot), before validation.
pub fn bound_loop_iterations(module: &mut Module, budget: u32) -> LoopBounds {
    let has_loops = module
        .functions
        .iter()
        .any(|(_, function)| block_has_loop(&function.body))
        || module
            .entry_points
            .iter()
            .any(|entry_point| block_has_loop(&entry_point.function.body));
    if !has_loops {
        return LoopBounds::default();
    }

    let u32_type = module.types.insert(
        Type {
            name: None,
            inner: TypeInner::Scalar(Scalar::U32),
        },
        Span::default(),
    );
    let zero = module
        .global_expressions
        .append(Expression::Literal(Literal::U32(0)), Span::default());
    let global = module.global_variables.append(
        GlobalVariable {
            name: Some(String::from(LOOP_BUDGET_GLOBAL)),
            space: AddressSpace::Private,
            binding: None,
            ty: u32_type,
            init: Some(zero),
            memory_decorations: Default::default(),
        },
        Span::default(),
    );

    let atomic_type = module.types.insert(
        Type {
            name: None,
            inner: TypeInner::Atomic(Scalar::U32),
        },
        Span::default(),
    );
    let fault_binding = next_free_group0_binding(module);
    let fault = module.global_variables.append(
        GlobalVariable {
            name: Some(String::from(LOOP_FAULT_GLOBAL)),
            // LOAD | STORE is `var<storage, read_write>`; the ATOMIC flag would
            // write naga's own `var<storage, atomic>`, which the browser's
            // WGSL compiler does not know.
            space: AddressSpace::Storage {
                access: StorageAccess::LOAD | StorageAccess::STORE,
            },
            binding: Some(ResourceBinding {
                group: 0,
                binding: fault_binding,
            }),
            ty: atomic_type,
            init: None,
            memory_decorations: Default::default(),
        },
        Span::default(),
    );

    let mut loops = 0;
    for (_, function) in module.functions.iter_mut() {
        loops += bound_function(function, global, fault, budget);
    }
    for entry_point in &mut module.entry_points {
        loops += bound_function(&mut entry_point.function, global, fault, budget);
    }
    LoopBounds {
        loops,
        fault_binding: Some(fault_binding),
    }
}

/// One past the highest `@group(0)` binding in use (uniforms, and the
/// textures `crate::uniform_layout::assign_texture_bindings` assigned).
fn next_free_group0_binding(module: &Module) -> u32 {
    module
        .global_variables
        .iter()
        .filter_map(|(_, var)| var.binding.as_ref())
        .filter(|binding| binding.group == 0)
        .map(|binding| binding.binding + 1)
        .max()
        .unwrap_or(0)
}

// ---- layer 1: static exit analysis ----------------------------------------

/// Every function some entry point can reach through `Call` statements,
/// in arena order (so the first refusal is deterministic).
fn reachable_functions(module: &Module) -> Vec<Handle<Function>> {
    let mut reached = vec![false; module.functions.len()];
    let mut worklist = Vec::new();
    for entry_point in &module.entry_points {
        collect_calls(&entry_point.function.body, &mut worklist);
    }
    while let Some(handle) = worklist.pop() {
        if core::mem::replace(&mut reached[handle.index()], true) {
            continue;
        }
        collect_calls(&module.functions[handle].body, &mut worklist);
    }
    module
        .functions
        .iter()
        .map(|(handle, _)| handle)
        .filter(|handle| reached[handle.index()])
        .collect()
}

fn collect_calls(block: &Block, out: &mut Vec<Handle<Function>>) {
    for statement in block.iter() {
        match statement {
            Statement::Call { function, .. } => out.push(*function),
            Statement::Block(inner) => collect_calls(inner, out),
            Statement::If { accept, reject, .. } => {
                collect_calls(accept, out);
                collect_calls(reject, out);
            }
            Statement::Switch { cases, .. } => {
                for case in cases {
                    collect_calls(&case.body, out);
                }
            }
            Statement::Loop {
                body, continuing, ..
            } => {
                collect_calls(body, out);
                collect_calls(continuing, out);
            }
            _ => {}
        }
    }
}

/// Walk a block looking for loops; refuse the first one with no exit.
fn check_block(
    module: &Module,
    function: &Function,
    block: &Block,
    unit: &AssembledGlsl,
) -> Result<(), String> {
    for (statement, span) in block.span_iter() {
        match statement {
            Statement::Block(inner) => check_block(module, function, inner, unit)?,
            Statement::If { accept, reject, .. } => {
                check_block(module, function, accept, unit)?;
                check_block(module, function, reject, unit)?;
            }
            Statement::Switch { cases, .. } => {
                for case in cases {
                    check_block(module, function, &case.body, unit)?;
                }
            }
            Statement::Loop {
                body,
                continuing,
                break_if,
            } => {
                // `break_if` is an exit unless it folds to a constant false.
                let break_if_exits =
                    break_if.is_some_and(|c| fold_bool(module, function, c) != Some(false));
                // naga forbids `return`, `discard`, and a `break` targeting
                // this loop inside `continuing`, so only the body can exit.
                if !break_if_exits && !block_has_exit(module, function, body, true) {
                    return Err(describe_unbounded_loop(function, *span, unit));
                }
                check_block(module, function, body, unit)?;
                check_block(module, function, continuing, unit)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// Does any path through `block` leave the enclosing loop?
///
/// `break_exits` is true while a `Break` still targets that loop; it turns
/// false inside a nested `Loop` or `Switch`, whose `Break`s target themselves.
/// `Return` and `Kill` exit from any depth.
fn block_has_exit(module: &Module, function: &Function, block: &Block, break_exits: bool) -> bool {
    block.iter().any(|statement| match statement {
        Statement::Break => break_exits,
        Statement::Return { .. } | Statement::Kill => true,
        Statement::Block(inner) => block_has_exit(module, function, inner, break_exits),
        Statement::If {
            condition,
            accept,
            reject,
        } => match fold_bool(module, function, *condition) {
            Some(true) => block_has_exit(module, function, accept, break_exits),
            Some(false) => block_has_exit(module, function, reject, break_exits),
            None => {
                block_has_exit(module, function, accept, break_exits)
                    || block_has_exit(module, function, reject, break_exits)
            }
        },
        Statement::Switch { cases, .. } => cases
            .iter()
            .any(|case| block_has_exit(module, function, &case.body, false)),
        Statement::Loop {
            body, continuing, ..
        } => {
            block_has_exit(module, function, body, false)
                || block_has_exit(module, function, continuing, false)
        }
        _ => false,
    })
}

/// Fold a function-scope boolean expression to a constant where the shape is
/// obvious: literals, `!` of a foldable, zero values, and module constants.
fn fold_bool(module: &Module, function: &Function, expr: Handle<Expression>) -> Option<bool> {
    match function.expressions[expr] {
        Expression::Literal(Literal::Bool(value)) => Some(value),
        Expression::ZeroValue(ty) if is_bool(module, ty) => Some(false),
        Expression::Unary {
            op: UnaryOperator::LogicalNot,
            expr,
        } => fold_bool(module, function, expr).map(|value| !value),
        Expression::Constant(constant) => fold_global_bool(module, module.constants[constant].init),
        _ => None,
    }
}

/// [`fold_bool`] over the module's global expression arena.
fn fold_global_bool(module: &Module, expr: Handle<Expression>) -> Option<bool> {
    match module.global_expressions[expr] {
        Expression::Literal(Literal::Bool(value)) => Some(value),
        Expression::ZeroValue(ty) if is_bool(module, ty) => Some(false),
        Expression::Unary {
            op: UnaryOperator::LogicalNot,
            expr,
        } => fold_global_bool(module, expr).map(|value| !value),
        _ => None,
    }
}

fn is_bool(module: &Module, ty: Handle<Type>) -> bool {
    matches!(module.types[ty].inner, TypeInner::Scalar(Scalar::BOOL))
}

/// The compile diagnostic: function, assembled-source line, and the line's
/// text, so the author sees the loop head rather than a naga handle.
/// The refusal text: a headline naming the function, the authored line and
/// the loop head, then the codespan-style marker block naga's own
/// diagnostics carry (`┌─ glsl:LINE:COL`, gutter line, carets), so the
/// Studio parser locates it by the same marker with no new shape to learn.
/// A loop outside the authored text (a canonical builtin, generated code)
/// keeps the headline and drops the location.
fn describe_unbounded_loop(function: &Function, span: Span, unit: &AssembledGlsl) -> String {
    const WHY: &str = "no path through the loop body breaks, returns or discards. The GPU \
                       tier has no fuel meter, so it refuses to compile a loop that can \
                       never exit.";
    let name = function.name.as_deref().unwrap_or("<entry point>");
    let authored = unit.to_authored(span);
    if !authored.is_defined() {
        let text = span
            .to_range()
            .map(|_| line_at(&unit.glsl, span.location(&unit.glsl).line_number))
            .unwrap_or_default();
        return format!("unbounded loop in `{name}` (`{}`): {WHY}", text.trim());
    }
    let source = unit.authored_text();
    let location = authored.location(source);
    let line = location.line_number;
    let col = location.line_position;
    let raw = line_at(source, line);
    let text = raw.trim();
    // Carets under the loop head, from the span's column to the end of its
    // first line (a multi-line loop body is not underlined).
    let head_len = (raw.len() + 1)
        .saturating_sub(col as usize)
        .min(location.length as usize)
        .max(1);
    let pad = " ".repeat(line.to_string().len() + 1);
    let caret_pad = " ".repeat(col.saturating_sub(1) as usize);
    let carets = "^".repeat(head_len);
    format!(
        "unbounded loop in `{name}` at line {line} (`{text}`): {WHY}\n\
         {pad}┌─ glsl:{line}:{col}\n\
         {pad}│\n\
         {line} │ {raw}\n\
         {pad}│ {caret_pad}{carets}\n"
    )
}

/// The 1-based `line` of `source`, or empty past the end.
fn line_at(source: &str, line: u32) -> &str {
    source
        .lines()
        .nth(line.saturating_sub(1) as usize)
        .unwrap_or_default()
}

// ---- layer 2: the injected back-edge budget --------------------------------

/// Handles a function shares across all its loops.
struct BudgetCharge {
    /// `&lp_gfx_loop_budget` (a `var<private>` pointer).
    pointer: Handle<Expression>,
    /// `&lp_gfx_loop_fault` (a `var<storage>` pointer to the atomic).
    fault: Handle<Expression>,
    /// The literal `1u`.
    one: Handle<Expression>,
    /// The literal budget.
    cap: Handle<Expression>,
    /// The literal `budget + 1`: the value the charge has exactly when an
    /// invocation crosses the budget, and never again.
    cap_plus_one: Handle<Expression>,
}

fn bound_function(
    function: &mut Function,
    global: Handle<GlobalVariable>,
    fault: Handle<GlobalVariable>,
    budget: u32,
) -> usize {
    if !block_has_loop(&function.body) {
        return 0;
    }
    let Function {
        expressions, body, ..
    } = function;
    // Pre-emit expressions (global pointers and literals) may sit anywhere in
    // the arena and need no `Emit` coverage.
    let charge = BudgetCharge {
        pointer: expressions.append(Expression::GlobalVariable(global), Span::default()),
        fault: expressions.append(Expression::GlobalVariable(fault), Span::default()),
        one: expressions.append(Expression::Literal(Literal::U32(1)), Span::default()),
        cap: expressions.append(Expression::Literal(Literal::U32(budget)), Span::default()),
        cap_plus_one: expressions.append(
            Expression::Literal(Literal::U32(budget.saturating_add(1))),
            Span::default(),
        ),
    };
    bound_block(body, expressions, &charge)
}

/// Charge every loop in `block` (recursively), returning how many.
fn bound_block(
    block: &mut Block,
    expressions: &mut Arena<Expression>,
    charge: &BudgetCharge,
) -> usize {
    let mut bounded = 0;
    for (statement, span) in block.span_iter_mut() {
        let span = span.map(|s| *s).unwrap_or_default();
        match statement {
            Statement::Block(inner) => bounded += bound_block(inner, expressions, charge),
            Statement::If { accept, reject, .. } => {
                bounded += bound_block(accept, expressions, charge);
                bounded += bound_block(reject, expressions, charge);
            }
            Statement::Switch { cases, .. } => {
                for case in cases {
                    bounded += bound_block(&mut case.body, expressions, charge);
                }
            }
            Statement::Loop {
                body,
                continuing,
                break_if,
            } => {
                bounded += bound_block(body, expressions, charge);
                bounded += bound_block(continuing, expressions, charge);
                charge_loop(continuing, break_if, expressions, charge, span);
                bounded += 1;
            }
            _ => {}
        }
    }
    bounded
}

/// Append to `continuing`: `budget = budget + 1u; if (budget == cap + 1u)
/// { atomicAdd(&fault, 1u); }` and make the loop `break if budget > cap`
/// (OR-ed onto an existing `break if`).
///
/// `continuing` runs once per back-edge — including the ones a `continue`
/// takes — and `break_if` is evaluated right after it, so the charge and the
/// check sit exactly where the LPVM's fuel decrement does. The flag is
/// charged on the crossing only, so an invocation counts once however many
/// loops it runs through afterwards (each of which breaks at once).
///
/// The order is load-bearing: the budget is **stored first and loaded
/// back** for both the crossing test and the `break if`. With the checks on
/// the pre-store sum (`let e = budget + 1u; …; budget = e; break if e >
/// cap`) Metal bounds the loop correctly but drops the guarded `atomicAdd`
/// entirely — a top-level loop never counted, a nested one did
/// (`docs/defects/2026-09-07-metal-drops-atomic-guarded-by-loop-exit-sum.md`).
/// Every shape whose `break if` reads the variable back after the store
/// counts; `tests/loop_fault.rs` holds it on a device.
fn charge_loop(
    continuing: &mut Block,
    break_if: &mut Option<Handle<Expression>>,
    expressions: &mut Arena<Expression>,
    charge: &BudgetCharge,
    span: Span,
) {
    // budget = budget + 1u;
    let load = expressions.append(
        Expression::Load {
            pointer: charge.pointer,
        },
        span,
    );
    let spent = expressions.append(
        Expression::Binary {
            op: BinaryOperator::Add,
            left: load,
            right: charge.one,
        },
        span,
    );
    continuing.push(Statement::Emit(Range::new_from_bounds(load, spent)), span);
    continuing.push(
        Statement::Store {
            pointer: charge.pointer,
            value: spent,
        },
        span,
    );
    // Then read it back for the checks (see the doc comment).
    let charged = expressions.append(
        Expression::Load {
            pointer: charge.pointer,
        },
        span,
    );
    let exhausted = expressions.append(
        Expression::Binary {
            op: BinaryOperator::Greater,
            left: charged,
            right: charge.cap,
        },
        span,
    );
    let crossed = expressions.append(
        Expression::Binary {
            op: BinaryOperator::Equal,
            left: charged,
            right: charge.cap_plus_one,
        },
        span,
    );
    let (condition, last) = match *break_if {
        Some(existing) => {
            let either = expressions.append(
                Expression::Binary {
                    op: BinaryOperator::LogicalOr,
                    left: existing,
                    right: exhausted,
                },
                span,
            );
            (either, either)
        }
        None => (exhausted, crossed),
    };
    continuing.push(Statement::Emit(Range::new_from_bounds(charged, last)), span);
    let mut count_fault = Block::new();
    count_fault.push(
        Statement::Atomic {
            pointer: charge.fault,
            fun: AtomicFunction::Add,
            value: charge.one,
            result: None,
        },
        span,
    );
    continuing.push(
        Statement::If {
            condition: crossed,
            accept: count_fault,
            reject: Block::new(),
        },
        span,
    );
    *break_if = Some(condition);
}

fn block_has_loop(block: &Block) -> bool {
    block.iter().any(|statement| match statement {
        Statement::Loop { .. } => true,
        Statement::Block(inner) => block_has_loop(inner),
        Statement::If { accept, reject, .. } => block_has_loop(accept) || block_has_loop(reject),
        Statement::Switch { cases, .. } => cases.iter().any(|case| block_has_loop(&case.body)),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUDGET: u32 = 100_000;

    // ---- layer 1: refusal -------------------------------------------------

    #[test]
    fn while_true_is_refused_and_named() {
        let (module, source) = parse_fragment(
            "float acc = 0.0;\n\
             while (true) { acc += 0.001; }\n\
             color = vec4(acc);\n",
        );
        let message = refuse_loops_without_exit(&module, &source).expect_err("refused");
        assert!(message.contains("unbounded loop"), "{message}");
        assert!(message.contains("`main`"), "names the function: {message}");
        assert!(
            message.contains("while (true)"),
            "quotes the loop head: {message}"
        );
        assert!(message.contains("fuel meter"), "says why: {message}");
    }

    #[test]
    fn bare_for_and_do_while_true_are_refused() {
        for body in [
            "for (;;) { acc += 0.001; }",
            "do { acc += 0.001; } while (true);",
            "while (!false) { acc += 0.001; }",
        ] {
            let (module, source) =
                parse_fragment(&format!("float acc = 0.0;\n{body}\ncolor = vec4(acc);\n"));
            refuse_loops_without_exit(&module, &source)
                .expect_err(&format!("`{body}` has no exit"));
        }
    }

    #[test]
    fn a_break_inside_a_nested_switch_does_not_exit_the_loop() {
        let (module, source) = parse_fragment(
            "int i = 0;\n\
             while (true) { switch (i) { case 0: break; default: i++; } }\n\
             color = vec4(float(i));\n",
        );
        refuse_loops_without_exit(&module, &source).expect_err("switch break stays in the loop");
    }

    #[test]
    fn a_break_inside_a_nested_loop_does_not_exit_the_outer_loop() {
        let (module, source) = parse_fragment(
            "int i = 0;\n\
             while (true) { for (int j = 0; j < 4; j++) { if (j == 2) break; } i++; }\n\
             color = vec4(float(i));\n",
        );
        refuse_loops_without_exit(&module, &source).expect_err("inner break stays inner");
    }

    #[test]
    fn loops_with_an_exit_on_some_path_are_accepted() {
        for body in [
            "for (int i = 0; i < 8; i++) { acc += 0.1; }",
            "while (acc < 1.0) { acc += 0.1; }",
            "while (true) { acc += 0.1; if (acc > 1.0) break; }",
            "while (true) { acc += 0.1; if (acc > 1.0) { color = vec4(acc); return; } }",
            "while (true) { acc += 0.1; if (acc > 1.0) discard; }",
            "while (true) { for (;;) { if (acc > 1.0) break; acc += 0.1; } if (acc > 2.0) break; }",
            "while (true) { switch (int(acc)) { case 0: acc += 0.5; break; default: return; } }",
        ] {
            let (module, source) =
                parse_fragment(&format!("float acc = 0.0;\n{body}\ncolor = vec4(acc);\n"));
            refuse_loops_without_exit(&module, &source)
                .unwrap_or_else(|e| panic!("`{body}` has an exit: {e}"));
        }
    }

    #[test]
    fn an_infinite_loop_inside_a_bounded_one_is_still_refused() {
        let (module, source) = parse_fragment(
            "float acc = 0.0;\n\
             for (int i = 0; i < 4; i++) { while (true) { acc += 0.1; } }\n\
             color = vec4(acc);\n",
        );
        refuse_loops_without_exit(&module, &source).expect_err("inner loop never exits");
    }

    /// The refusal reports the AUTHORED line with naga's marker shape: a
    /// unit whose authored range starts after a prefix maps the loop back
    /// to its line within that range.
    #[test]
    fn refusal_marks_the_authored_line_with_the_codespan_marker() {
        let helpers = "float spin() {\n\
                       \x20   float acc = 0.0;\n\
                       \x20   while (true) { acc += 0.1; }\n\
                       \x20   return acc;\n\
                       }\n";
        let (module, unit) = parse_fragment_with_helpers(helpers, "color = vec4(spin());\n");
        // Treat the helper text as the authored region: the version and
        // output-declaration lines before it play the assembled prefix.
        let start = unit.glsl.find(helpers).expect("helpers spliced verbatim");
        let unit = AssembledGlsl {
            authored: start..start + helpers.len(),
            glsl: unit.glsl,
        };
        let message = refuse_loops_without_exit(&module, &unit).expect_err("refused");
        assert!(
            message.starts_with(
                "unbounded loop in `spin` at line 3 (`while (true) { acc += 0.1; }`): no path"
            ),
            "{message}"
        );
        assert!(message.contains("\n  ┌─ glsl:3:5\n"), "{message}");
        assert!(
            message.contains("\n3 │     while (true) { acc += 0.1; }\n"),
            "{message}"
        );
        assert!(message.contains("\n  │     ^^^^^"), "carets: {message}");
    }

    /// A loop outside the authored range (a builtin, generated code) is
    /// still refused and named, without a location that would point the
    /// editor at the wrong line.
    #[test]
    fn refusal_outside_the_authored_range_carries_no_location() {
        let helpers =
            "float spin() { float acc = 0.0; while (true) { acc += 0.1; } return acc; }\n";
        let main_body = "color = vec4(spin());\n";
        let (module, unit) = parse_fragment_with_helpers(helpers, main_body);
        let start = unit.glsl.find(main_body).expect("body spliced verbatim");
        let unit = AssembledGlsl {
            authored: start..start + main_body.len(),
            glsl: unit.glsl,
        };
        let message = refuse_loops_without_exit(&module, &unit).expect_err("refused");
        assert!(message.contains("`spin`"), "{message}");
        assert!(message.contains("while (true)"), "{message}");
        assert!(!message.contains("at line"), "{message}");
        assert!(!message.contains("┌─ glsl:"), "{message}");
    }

    #[test]
    fn a_loop_in_a_helper_function_is_checked_too() {
        let (module, source) = parse_fragment_with_helpers(
            "float spin() { float acc = 0.0; while (true) { acc += 0.1; } return acc; }\n",
            "color = vec4(spin());\n",
        );
        let message = refuse_loops_without_exit(&module, &source).expect_err("refused");
        assert!(message.contains("`spin`"), "names the helper: {message}");
    }

    #[test]
    fn an_uncalled_helper_with_an_infinite_loop_does_not_refuse_the_unit() {
        let (module, source) = parse_fragment_with_helpers(
            "float spin() { float acc = 0.0; while (true) { acc += 0.1; } return acc; }\n",
            "color = vec4(sin(gl_FragCoord.x));\n",
        );
        assert!(
            module
                .functions
                .iter()
                .any(|(_, f)| f.name.as_deref() == Some("spin")),
            "glsl-in keeps the uncalled helper, so reachability is what spares it"
        );
        refuse_loops_without_exit(&module, &source)
            .expect("a loop nothing calls cannot run; the LPVM tiers compile this unit");
    }

    #[test]
    fn a_helper_reached_through_another_helper_is_checked() {
        let (module, source) = parse_fragment_with_helpers(
            "float spin() { float acc = 0.0; while (true) { acc += 0.1; } return acc; }\n\
             float via() { return spin() * 2.0; }\n",
            "color = vec4(via());\n",
        );
        let message = refuse_loops_without_exit(&module, &source).expect_err("refused");
        assert!(message.contains("`spin`"), "{message}");
    }

    // ---- layer 2: the budget ---------------------------------------------

    #[test]
    fn every_loop_is_charged_in_continuing_and_breaks_when_spent() {
        let (mut module, _) = parse_fragment(
            "float acc = 0.0;\n\
             for (int i = 0; i < 8; i++) { acc += 0.1; }\n\
             while (acc < 4.0) { acc *= 1.5; }\n\
             color = vec4(acc);\n",
        );
        let bounds = bound_loop_iterations(&mut module, BUDGET);
        assert_eq!(bounds.loops, 2);
        assert_eq!(bounds.fault_binding, Some(0), "no other group-0 bindings");
        let wgsl = validate_and_write(&module);
        assert!(
            wgsl.contains(&format!("var<private> {LOOP_BUDGET_GLOBAL}: u32 = 0u;")),
            "per-invocation budget global:\n{wgsl}"
        );
        assert_eq!(
            wgsl.matches("break if").count(),
            2,
            "one break-if per loop:\n{wgsl}"
        );
        assert_eq!(
            wgsl.matches(&format!("{LOOP_BUDGET_GLOBAL} = ")).count(),
            2,
            "one charge per loop:\n{wgsl}"
        );
        assert!(
            wgsl.contains(&format!("> {BUDGET}u")),
            "the budget is the cap:\n{wgsl}"
        );
    }

    #[test]
    fn the_charge_lands_in_continuing_after_the_authored_increment_and_is_read_back() {
        let (mut module, _) = parse_fragment(
            "float acc = 0.0;\n\
             for (int i = 0; i < 8; i++) { acc += 0.1; }\n\
             color = vec4(acc);\n",
        );
        bound_loop_iterations(&mut module, BUDGET);
        let function = looping_function(&module);
        let (continuing, break_if) = find_first_loop(&function.body).expect("one loop");
        assert!(break_if.is_some(), "break_if is set");
        let statements: Vec<&Statement> = continuing.iter().collect();
        let (last_store, _) = statements
            .iter()
            .enumerate()
            .rev()
            .find(|(_, s)| matches!(s, Statement::Store { .. }))
            .expect("charge store");
        let stores = statements
            .iter()
            .filter(|s| matches!(s, Statement::Store { .. }))
            .count();
        assert_eq!(stores, 2, "the authored `i++` store precedes the charge");
        // After the charge: one `Emit` (the read-back and the checks) and the
        // fault guard, nothing else. The `break if` reads the read-back, not
        // the sum that was stored — the Metal miscompile guard.
        assert!(
            matches!(statements[last_store + 1], Statement::Emit(_)),
            "the checks are emitted after the store"
        );
        assert!(
            matches!(statements[last_store + 2], Statement::If { .. }),
            "the fault guard follows"
        );
        assert_eq!(statements.len(), last_store + 3);
        let condition = break_if.expect("break_if");
        let Expression::Binary { left, .. } = function.expressions[condition] else {
            panic!("break_if is a comparison");
        };
        assert!(
            matches!(function.expressions[left], Expression::Load { .. }),
            "the break if compares the budget read back after the store"
        );
    }

    #[test]
    fn the_fault_flag_is_a_group0_storage_atomic_charged_once_on_the_crossing() {
        let (mut module, _) = parse_fragment_with_helpers(
            "float spin(float x) { for (int i = 0; i < 3; i++) { x *= 2.0; } return x; }\n",
            "float acc = 0.0;\n\
             for (int i = 0; i < 2; i++) { while (acc < 4.0) { acc += spin(0.5); } }\n\
             color = vec4(acc);\n",
        );
        let bounds = bound_loop_iterations(&mut module, BUDGET);
        assert_eq!(bounds.loops, 3);
        let wgsl = validate_and_write(&module);
        assert_eq!(
            bounds.fault_binding,
            Some(0),
            "the next free group-0 binding"
        );
        assert!(
            wgsl.contains(&format!(
                "var<storage, read_write> {LOOP_FAULT_GLOBAL}: atomic<u32>;"
            )),
            "a read_write storage atomic (standard WGSL, not naga's `atomic` access):\n{wgsl}"
        );
        assert_eq!(
            wgsl.matches(&format!("var<storage, read_write> {LOOP_FAULT_GLOBAL}"))
                .count(),
            1,
            "one shared flag:\n{wgsl}"
        );
        assert_eq!(
            wgsl.matches("atomicAdd(").count(),
            3,
            "every loop can count the crossing:\n{wgsl}"
        );
        assert!(
            wgsl.contains(&format!("== {}u", BUDGET + 1)),
            "the flag is charged on the crossing only:\n{wgsl}"
        );
        // On the IR: the `if` guarding the atomic sits between the `Emit`
        // and the budget `Store`, and its condition is the crossing test.
        let function = looping_function(&module);
        let (continuing, _) = find_first_loop(&function.body).expect("a loop");
        let guard = continuing
            .iter()
            .find_map(|s| match s {
                Statement::If {
                    condition, accept, ..
                } => Some((*condition, accept)),
                _ => None,
            })
            .expect("the fault guard");
        assert!(
            matches!(
                function.expressions[guard.0],
                Expression::Binary {
                    op: BinaryOperator::Equal,
                    ..
                }
            ),
            "guarded by the crossing equality"
        );
        assert!(
            matches!(
                guard.1.iter().next(),
                Some(Statement::Atomic {
                    fun: AtomicFunction::Add,
                    result: None,
                    ..
                })
            ),
            "one atomic add, result unused"
        );
    }

    #[test]
    fn the_fault_flag_takes_the_binding_after_the_authored_uniforms() {
        let (mut module, _) = parse_fragment_with_helpers(
            "layout(binding = 2) uniform vec2 sz;\n",
            "float acc = 0.0;\n\
             for (int i = 0; i < 8; i++) { acc += sz.x; }\n\
             color = vec4(acc);\n",
        );
        let bounds = bound_loop_iterations(&mut module, BUDGET);
        assert_eq!(bounds.fault_binding, Some(3));
        let wgsl = validate_and_write(&module);
        assert!(
            wgsl.contains("@binding(3)"),
            "flag bound after the uniform:\n{wgsl}"
        );
    }

    #[test]
    fn an_existing_break_if_is_kept_and_ored_with_the_budget() {
        let (mut module, _) = parse_fragment(
            "float acc = 0.0;\n\
             for (int i = 0; i < 8; i++) { acc += 0.1; }\n\
             color = vec4(acc);\n",
        );
        // Give the loop a break_if of its own first (glsl-in never emits one).
        {
            let Function {
                expressions, body, ..
            } = looping_function_mut(&mut module);
            let flag =
                expressions.append(Expression::Literal(Literal::Bool(false)), Span::default());
            let (_, break_if) = find_first_loop_mut(body).expect("one loop");
            *break_if = Some(flag);
        }
        bound_loop_iterations(&mut module, BUDGET);
        let function = looping_function(&module);
        let (_, break_if) = find_first_loop(&function.body).expect("one loop");
        let condition = break_if.expect("break_if kept");
        assert!(
            matches!(
                function.expressions[condition],
                Expression::Binary {
                    op: BinaryOperator::LogicalOr,
                    ..
                }
            ),
            "existing break_if OR budget"
        );
        validate_and_write(&module);
    }

    #[test]
    fn loops_in_helpers_and_nested_loops_are_all_charged() {
        let (mut module, _) = parse_fragment_with_helpers(
            "float spin(float x) { for (int i = 0; i < 3; i++) { x *= 2.0; } return x; }\n",
            "float acc = 0.0;\n\
             for (int i = 0; i < 2; i++) { for (int j = 0; j < 2; j++) { acc += spin(0.5); } }\n\
             color = vec4(acc);\n",
        );
        assert_eq!(bound_loop_iterations(&mut module, BUDGET).loops, 3);
        let wgsl = validate_and_write(&module);
        assert_eq!(wgsl.matches("break if").count(), 3, "{wgsl}");
        assert_eq!(
            wgsl.matches(&format!("var<private> {LOOP_BUDGET_GLOBAL}"))
                .count(),
            1,
            "one shared budget global:\n{wgsl}"
        );
    }

    #[test]
    fn modules_without_loops_are_untouched() {
        let (mut module, _) = parse_fragment("color = vec4(sin(gl_FragCoord.x));\n");
        let globals_before = module.global_variables.len();
        let expressions_before = expression_count(&module);
        assert_eq!(
            bound_loop_iterations(&mut module, BUDGET),
            LoopBounds::default()
        );
        assert_eq!(module.global_variables.len(), globals_before);
        assert_eq!(expression_count(&module), expressions_before);
        let wgsl = validate_and_write(&module);
        assert!(!wgsl.contains(LOOP_BUDGET_GLOBAL), "{wgsl}");
        assert!(!wgsl.contains(LOOP_FAULT_GLOBAL), "no flag either:\n{wgsl}");
    }

    // ---- helpers ------------------------------------------------------------

    fn parse_fragment(main_body: &str) -> (naga::Module, AssembledGlsl) {
        parse_fragment_with_helpers("", main_body)
    }

    /// The whole source counts as authored (`AssembledGlsl::unassembled`);
    /// the authored-range tests above narrow it.
    fn parse_fragment_with_helpers(
        helpers: &str,
        main_body: &str,
    ) -> (naga::Module, AssembledGlsl) {
        let source = format!(
            "#version 450 core\n\
             layout(location = 0) out vec4 color;\n\
             {helpers}\
             void main() {{\n{main_body}}}\n"
        );
        let mut frontend = naga::front::glsl::Frontend::default();
        let options = naga::front::glsl::Options::from(naga::ShaderStage::Fragment);
        let module = frontend
            .parse(&options, &source)
            .unwrap_or_else(|e| panic!("glsl parses: {}", e.emit_to_string(&source)));
        (module, AssembledGlsl::unassembled(source))
    }

    fn validate_and_write(module: &naga::Module) -> String {
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        );
        let info = validator
            .validate(module)
            .unwrap_or_else(|e| panic!("module validates: {e:?}"));
        naga::back::wgsl::write_string(module, &info, naga::back::wgsl::WriterFlags::empty())
            .expect("wgsl-out")
    }

    /// glsl-in lowers the authored `main` to a plain function the entry
    /// point wraps, so the loops live in `module.functions`, not the entry.
    fn looping_function(module: &naga::Module) -> &Function {
        module
            .functions
            .iter()
            .map(|(_, f)| f)
            .chain(module.entry_points.iter().map(|ep| &ep.function))
            .find(|f| block_has_loop(&f.body))
            .expect("a function with a loop")
    }

    fn looping_function_mut(module: &mut naga::Module) -> &mut Function {
        module
            .functions
            .iter_mut()
            .map(|(_, f)| f)
            .chain(module.entry_points.iter_mut().map(|ep| &mut ep.function))
            .find(|f| block_has_loop(&f.body))
            .expect("a function with a loop")
    }

    fn expression_count(module: &naga::Module) -> usize {
        module
            .functions
            .iter()
            .map(|(_, f)| f.expressions.len())
            .chain(
                module
                    .entry_points
                    .iter()
                    .map(|ep| ep.function.expressions.len()),
            )
            .sum()
    }

    fn find_first_loop(block: &Block) -> Option<(&Block, Option<Handle<Expression>>)> {
        block.iter().find_map(|statement| match statement {
            Statement::Loop {
                continuing,
                break_if,
                ..
            } => Some((continuing, *break_if)),
            Statement::Block(inner) => find_first_loop(inner),
            _ => None,
        })
    }

    fn find_first_loop_mut(
        block: &mut Block,
    ) -> Option<(&mut Block, &mut Option<Handle<Expression>>)> {
        block.iter_mut().find_map(|statement| match statement {
            Statement::Loop {
                continuing,
                break_if,
                ..
            } => Some((continuing, break_if)),
            Statement::Block(inner) => find_first_loop_mut(inner),
            _ => None,
        })
    }
}
