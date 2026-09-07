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
//!    function and the assembled-source line. Constant `if` conditions are
//!    folded first, because glsl-in lowers `while (true)` to a loop whose
//!    body opens with `if (!true) { break; }` — the demo's exit is
//!    syntactically present and semantically dead. The check is
//!    conservative the safe way round: it only refuses loops with *no*
//!    exit; a data-dependent runaway compiles and is left to layer 2.
//! 2. [`bound_loop_iterations`] — every remaining loop charges one unit of a
//!    per-invocation `var<private>` budget in its `continuing` block and
//!    `break if`s once the budget is spent. The unit is the loop back-edge,
//!    exactly the fuel meter's, and the budget is the same constant. Unlike
//!    the LPVM, the GPU cannot trap: an exhausted budget exits the loop and
//!    the invocation completes with whatever it has — bounded, not faulted.
//!
//! Neither layer rebuilds an expression arena: the budget's expressions are
//! appended (operands precede uses by construction) and only `continuing`
//! blocks and `break_if` slots are touched, so existing `Emit` ranges stay
//! valid.

use naga::{
    AddressSpace, Arena, BinaryOperator, Block, Expression, Function, GlobalVariable, Handle,
    Literal, Module, Range, Scalar, Span, Statement, Type, TypeInner, UnaryOperator,
};

/// Name of the injected per-invocation budget global (`var<private>`).
pub const LOOP_BUDGET_GLOBAL: &str = "lp_gfx_loop_budget";

/// Refuse any loop whose body has no exit on any path.
///
/// `source` is the text the module was parsed from (the assembled GLSL), used
/// to name the offending line. Call after `glsl-in`, before validation.
pub fn refuse_loops_without_exit(module: &Module, source: &str) -> Result<(), String> {
    for (_, function) in module.functions.iter() {
        check_block(module, function, &function.body, source)?;
    }
    for entry_point in &module.entry_points {
        check_block(
            module,
            &entry_point.function,
            &entry_point.function.body,
            source,
        )?;
    }
    Ok(())
}

/// Charge every loop one unit of a per-invocation budget per back-edge and
/// break out once `budget` units are spent.
///
/// Returns the number of loops bounded. A module without loops is left
/// untouched (no global, no expressions). Call after
/// [`refuse_loops_without_exit`], before validation.
pub fn bound_loop_iterations(module: &mut Module, budget: u32) -> usize {
    let has_loops = module
        .functions
        .iter()
        .any(|(_, function)| block_has_loop(&function.body))
        || module
            .entry_points
            .iter()
            .any(|entry_point| block_has_loop(&entry_point.function.body));
    if !has_loops {
        return 0;
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

    let mut bounded = 0;
    for (_, function) in module.functions.iter_mut() {
        bounded += bound_function(function, global, budget);
    }
    for entry_point in &mut module.entry_points {
        bounded += bound_function(&mut entry_point.function, global, budget);
    }
    bounded
}

// ---- layer 1: static exit analysis ----------------------------------------

/// Walk a block looking for loops; refuse the first one with no exit.
fn check_block(
    module: &Module,
    function: &Function,
    block: &Block,
    source: &str,
) -> Result<(), String> {
    for (statement, span) in block.span_iter() {
        match statement {
            Statement::Block(inner) => check_block(module, function, inner, source)?,
            Statement::If { accept, reject, .. } => {
                check_block(module, function, accept, source)?;
                check_block(module, function, reject, source)?;
            }
            Statement::Switch { cases, .. } => {
                for case in cases {
                    check_block(module, function, &case.body, source)?;
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
                    return Err(describe_unbounded_loop(function, *span, source));
                }
                check_block(module, function, body, source)?;
                check_block(module, function, continuing, source)?;
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
fn describe_unbounded_loop(function: &Function, span: Span, source: &str) -> String {
    let name = function.name.as_deref().unwrap_or("<entry point>");
    let location = span.location(source);
    let line = location.line_number;
    let text = source
        .lines()
        .nth(line.saturating_sub(1) as usize)
        .map(str::trim)
        .unwrap_or_default();
    format!(
        "unbounded loop in `{name}` at line {line} (`{text}`): no path through the loop body \
         breaks, returns or discards. The GPU tier has no fuel meter, so it refuses to \
         compile a loop that can never exit."
    )
}

// ---- layer 2: the injected back-edge budget --------------------------------

/// Handles a function shares across all its loops.
struct BudgetCharge {
    /// `&lp_gfx_loop_budget` (a `var<private>` pointer).
    pointer: Handle<Expression>,
    /// The literal `1u`.
    one: Handle<Expression>,
    /// The literal budget.
    cap: Handle<Expression>,
}

fn bound_function(function: &mut Function, global: Handle<GlobalVariable>, budget: u32) -> usize {
    if !block_has_loop(&function.body) {
        return 0;
    }
    let Function {
        expressions, body, ..
    } = function;
    // Pre-emit expressions (a global pointer and literals) may sit anywhere in
    // the arena and need no `Emit` coverage.
    let charge = BudgetCharge {
        pointer: expressions.append(Expression::GlobalVariable(global), Span::default()),
        one: expressions.append(Expression::Literal(Literal::U32(1)), Span::default()),
        cap: expressions.append(Expression::Literal(Literal::U32(budget)), Span::default()),
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

/// Append to `continuing`: `budget = budget + 1u;` and make the loop
/// `break if budget > cap` (OR-ed onto an existing `break if`).
///
/// `continuing` runs once per back-edge — including the ones a `continue`
/// takes — and `break_if` is evaluated right after it, so the charge and the
/// check sit exactly where the LPVM's fuel decrement does.
fn charge_loop(
    continuing: &mut Block,
    break_if: &mut Option<Handle<Expression>>,
    expressions: &mut Arena<Expression>,
    charge: &BudgetCharge,
    span: Span,
) {
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
    let exhausted = expressions.append(
        Expression::Binary {
            op: BinaryOperator::Greater,
            left: spent,
            right: charge.cap,
        },
        span,
    );
    let condition = match *break_if {
        Some(existing) => expressions.append(
            Expression::Binary {
                op: BinaryOperator::LogicalOr,
                left: existing,
                right: exhausted,
            },
            span,
        ),
        None => exhausted,
    };
    continuing.push(
        Statement::Emit(Range::new_from_bounds(load, condition)),
        span,
    );
    continuing.push(
        Statement::Store {
            pointer: charge.pointer,
            value: spent,
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

    #[test]
    fn a_loop_in_a_helper_function_is_checked_too() {
        let (module, source) = parse_fragment_with_helpers(
            "float spin() { float acc = 0.0; while (true) { acc += 0.1; } return acc; }\n",
            "color = vec4(spin());\n",
        );
        let message = refuse_loops_without_exit(&module, &source).expect_err("refused");
        assert!(message.contains("`spin`"), "names the helper: {message}");
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
        assert_eq!(bound_loop_iterations(&mut module, BUDGET), 2);
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
    fn the_charge_lands_in_continuing_after_the_authored_increment() {
        let (mut module, _) = parse_fragment(
            "float acc = 0.0;\n\
             for (int i = 0; i < 8; i++) { acc += 0.1; }\n\
             color = vec4(acc);\n",
        );
        bound_loop_iterations(&mut module, BUDGET);
        let function = looping_function(&module);
        let (continuing, break_if) = find_first_loop(&function.body).expect("one loop");
        assert!(break_if.is_some(), "break_if is set");
        let (last_store, _) = continuing
            .iter()
            .enumerate()
            .rev()
            .find(|(_, s)| matches!(s, Statement::Store { .. }))
            .expect("charge store");
        assert_eq!(
            last_store + 1,
            continuing.len(),
            "the charge is the last statement of continuing"
        );
        let stores = continuing
            .iter()
            .filter(|s| matches!(s, Statement::Store { .. }))
            .count();
        assert_eq!(stores, 2, "the authored `i++` store precedes the charge");
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
        assert_eq!(bound_loop_iterations(&mut module, BUDGET), 3);
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
        assert_eq!(bound_loop_iterations(&mut module, BUDGET), 0);
        assert_eq!(module.global_variables.len(), globals_before);
        assert_eq!(expression_count(&module), expressions_before);
        let wgsl = validate_and_write(&module);
        assert!(!wgsl.contains(LOOP_BUDGET_GLOBAL), "{wgsl}");
    }

    // ---- helpers ------------------------------------------------------------

    fn parse_fragment(main_body: &str) -> (naga::Module, String) {
        parse_fragment_with_helpers("", main_body)
    }

    fn parse_fragment_with_helpers(helpers: &str, main_body: &str) -> (naga::Module, String) {
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
        (module, source)
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
