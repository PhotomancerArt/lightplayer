//! Bounded-tanh naga IR pass.
//!
//! Metal's fast-math `tanh` overflows to NaN for |x| ≳ 89 where the Q32
//! device path saturates (M3 spike finding, `nan_probe`). This pass rewrites
//! every `tanh(x)` in the module to `tanh(clamp(x, -20.0, 20.0))` — at the
//! IR level, after `glsl-in`, so it uniformly covers authored code *and* the
//! spliced canonical prelude. `tanh(±20)` is already ±1.0 at f32 precision,
//! so the clamp is exact for all representable outputs.
//!
//! naga expression arenas require operands to precede their uses, so
//! inserting the clamp rebuilds each affected function's expression arena
//! and remaps every expression/statement handle (monotonic old → new map).
//! Module-level constant expressions are not rewritten: they are evaluated
//! host-side by naga's constant evaluator, not by the GPU's fast-math
//! libcall, so the Metal overflow cannot occur there.

use std::collections::HashMap;

use naga::front::Typifier;
use naga::proc::ResolveContext;
use naga::{Expression, Handle, MathFunction, Range, Span, Statement, VectorSize};

/// Clamp bound: `tanh(20.0)` rounds to `1.0` in f32, so outputs are
/// unchanged for every input the clamp affects.
const TANH_BOUND: f32 = 20.0;

/// Rewrite every `tanh(x)` to `tanh(clamp(x, -TANH_BOUND, TANH_BOUND))`.
///
/// Returns the number of rewritten `tanh` call sites. Call between
/// `glsl-in` and validation.
pub fn bound_tanh(module: &mut naga::Module) -> Result<usize, String> {
    // Pass 1 (immutable): resolve each tanh argument's vector size so the
    // clamp bounds can be splatted to match (WGSL clamp is same-type only).
    let mut function_sizes = Vec::new();
    for (handle, function) in module.functions.iter() {
        if let Some(sizes) = tanh_arg_sizes(module, function)? {
            function_sizes.push((handle, sizes));
        }
    }
    let mut entry_point_sizes = Vec::new();
    for (index, entry_point) in module.entry_points.iter().enumerate() {
        if let Some(sizes) = tanh_arg_sizes(module, &entry_point.function)? {
            entry_point_sizes.push((index, sizes));
        }
    }

    // Pass 2 (mutable): rebuild the affected functions' expression arenas.
    let mut rewritten = 0;
    for (handle, sizes) in function_sizes {
        rewritten += rewrite_function(&mut module.functions[handle], &sizes);
    }
    for (index, sizes) in entry_point_sizes {
        rewritten += rewrite_function(&mut module.entry_points[index].function, &sizes);
    }
    Ok(rewritten)
}

/// Argument vector sizes per tanh expression (`None` value = scalar), or
/// `None` when the function contains no tanh at all.
type TanhArgSizes = HashMap<Handle<Expression>, Option<VectorSize>>;

fn tanh_arg_sizes(
    module: &naga::Module,
    function: &naga::Function,
) -> Result<Option<TanhArgSizes>, String> {
    let tanh_args: Vec<(Handle<Expression>, Handle<Expression>)> = function
        .expressions
        .iter()
        .filter_map(|(handle, expr)| match *expr {
            Expression::Math {
                fun: MathFunction::Tanh,
                arg,
                ..
            } => Some((handle, arg)),
            _ => None,
        })
        .collect();
    if tanh_args.is_empty() {
        return Ok(None);
    }

    let resolve_ctx =
        ResolveContext::with_locals(module, &function.local_variables, &function.arguments);
    let mut typifier = Typifier::new();
    let mut sizes = TanhArgSizes::new();
    for (tanh, arg) in tanh_args {
        typifier
            .grow(arg, &function.expressions, &resolve_ctx)
            .map_err(|e| format!("bounded-tanh pass: resolve tanh argument type: {e}"))?;
        let size = match *typifier.get(arg, &module.types) {
            naga::TypeInner::Vector { size, .. } => Some(size),
            _ => None,
        };
        sizes.insert(tanh, size);
    }
    Ok(Some(sizes))
}

/// Rebuild `function.expressions` with a `clamp` inserted before every tanh,
/// remapping every handle in expressions, statements, named expressions, and
/// local-variable initializers. Returns the number of rewritten tanh sites.
fn rewrite_function(function: &mut naga::Function, sizes: &TanhArgSizes) -> usize {
    let mut old = core::mem::take(&mut function.expressions);
    let new = &mut function.expressions;

    // Literal clamp bounds go first: literals are constant expressions that
    // need no `Emit` coverage and may precede everything.
    let lo = new.append(
        Expression::Literal(naga::Literal::F32(-TANH_BOUND)),
        Span::default(),
    );
    let hi = new.append(
        Expression::Literal(naga::Literal::F32(TANH_BOUND)),
        Span::default(),
    );

    // Old handle index → new handle of the same expression…
    let mut primary: Vec<Handle<Expression>> = Vec::with_capacity(old.len());
    // …and → first new handle produced while copying it (differs for tanh:
    // the inserted splats/clamp; used to extend `Emit` range starts).
    let mut first: Vec<Handle<Expression>> = Vec::with_capacity(old.len());
    let mut rewritten = 0;

    for (old_handle, mut expr, span) in old.drain() {
        remap_expression(&mut expr, &primary);
        let is_tanh = matches!(
            expr,
            Expression::Math {
                fun: MathFunction::Tanh,
                ..
            }
        );
        if is_tanh {
            let Expression::Math { arg, .. } = &mut expr else {
                unreachable!("matched Math above");
            };
            let size = sizes.get(&old_handle).copied().flatten();
            let (mut low, mut high) = (lo, hi);
            let mut first_inserted = None;
            if let Some(size) = size {
                low = new.append(Expression::Splat { size, value: lo }, span);
                high = new.append(Expression::Splat { size, value: hi }, span);
                first_inserted = Some(low);
            }
            let clamp = new.append(
                Expression::Math {
                    fun: MathFunction::Clamp,
                    arg: *arg,
                    arg1: Some(low),
                    arg2: Some(high),
                    arg3: None,
                },
                span,
            );
            *arg = clamp;
            let tanh = new.append(expr, span);
            first.push(first_inserted.unwrap_or(clamp));
            primary.push(tanh);
            rewritten += 1;
        } else {
            let handle = new.append(expr, span);
            first.push(handle);
            primary.push(handle);
        }
    }

    for statement in function.body.iter_mut() {
        remap_statement(statement, &primary, &first);
    }
    let named = core::mem::take(&mut function.named_expressions);
    function.named_expressions = named
        .into_iter()
        .map(|(handle, name)| (primary[handle.index()], name))
        .collect();
    for (_, local) in function.local_variables.iter_mut() {
        if let Some(init) = &mut local.init {
            *init = primary[init.index()];
        }
    }
    rewritten
}

/// Remap one expression's operand handles (operands always precede the
/// expression, so `primary` already covers them).
fn remap_expression(expr: &mut Expression, primary: &[Handle<Expression>]) {
    let map = |handle: &mut Handle<Expression>| *handle = primary[handle.index()];
    let map_opt = |handle: &mut Option<Handle<Expression>>| {
        if let Some(handle) = handle {
            *handle = primary[handle.index()];
        }
    };
    match expr {
        Expression::Literal(_)
        | Expression::Constant(_)
        | Expression::Override(_)
        | Expression::ZeroValue(_)
        | Expression::FunctionArgument(_)
        | Expression::GlobalVariable(_)
        | Expression::LocalVariable(_)
        | Expression::CallResult(_)
        | Expression::AtomicResult { .. }
        | Expression::WorkGroupUniformLoadResult { .. }
        | Expression::RayQueryProceedResult
        | Expression::SubgroupBallotResult
        | Expression::SubgroupOperationResult { .. } => {}
        Expression::Compose { components, .. } => {
            for component in components {
                map(component);
            }
        }
        Expression::Access { base, index } => {
            map(base);
            map(index);
        }
        Expression::AccessIndex { base, .. } => map(base),
        Expression::Splat { value, .. } => map(value),
        Expression::Swizzle { vector, .. } => map(vector),
        Expression::Load { pointer } => map(pointer),
        Expression::ImageSample {
            image,
            sampler,
            coordinate,
            array_index,
            offset,
            level,
            depth_ref,
            ..
        } => {
            map(image);
            map(sampler);
            map(coordinate);
            map_opt(array_index);
            map_opt(offset);
            match level {
                naga::SampleLevel::Auto | naga::SampleLevel::Zero => {}
                naga::SampleLevel::Exact(expr) | naga::SampleLevel::Bias(expr) => map(expr),
                naga::SampleLevel::Gradient { x, y } => {
                    map(x);
                    map(y);
                }
            }
            map_opt(depth_ref);
        }
        Expression::ImageLoad {
            image,
            coordinate,
            array_index,
            sample,
            level,
        } => {
            map(image);
            map(coordinate);
            map_opt(array_index);
            map_opt(sample);
            map_opt(level);
        }
        Expression::ImageQuery { image, query } => {
            map(image);
            if let naga::ImageQuery::Size { level } = query {
                map_opt(level);
            }
        }
        Expression::Unary { expr, .. } => map(expr),
        Expression::Binary { left, right, .. } => {
            map(left);
            map(right);
        }
        Expression::Select {
            condition,
            accept,
            reject,
        } => {
            map(condition);
            map(accept);
            map(reject);
        }
        Expression::Derivative { expr, .. } => map(expr),
        Expression::Relational { argument, .. } => map(argument),
        Expression::Math {
            arg,
            arg1,
            arg2,
            arg3,
            ..
        } => {
            map(arg);
            map_opt(arg1);
            map_opt(arg2);
            map_opt(arg3);
        }
        Expression::As { expr, .. } => map(expr),
        Expression::ArrayLength(expr) => map(expr),
        Expression::RayQueryVertexPositions { query, .. } => map(query),
        Expression::RayQueryGetIntersection { query, .. } => map(query),
        Expression::CooperativeLoad { data, .. } => {
            map(&mut data.pointer);
            map(&mut data.stride);
        }
        Expression::CooperativeMultiplyAdd { a, b, c } => {
            map(a);
            map(b);
            map(c);
        }
    }
}

/// Remap one statement's expression handles, recursing into nested blocks.
/// `Emit` range starts use the `first` map so inserted clamp chains stay
/// covered by their original emit range.
fn remap_statement(
    statement: &mut Statement,
    primary: &[Handle<Expression>],
    first: &[Handle<Expression>],
) {
    let map = |handle: &mut Handle<Expression>| *handle = primary[handle.index()];
    let map_opt = |handle: &mut Option<Handle<Expression>>| {
        if let Some(handle) = handle {
            *handle = primary[handle.index()];
        }
    };
    let map_block = |block: &mut naga::Block| {
        for statement in block.iter_mut() {
            remap_statement(statement, primary, first);
        }
    };
    match statement {
        Statement::Emit(range) => {
            if let Some((start, end)) = range.first_and_last() {
                *range = Range::new_from_bounds(first[start.index()], primary[end.index()]);
            }
        }
        Statement::Block(block) => map_block(block),
        Statement::If {
            condition,
            accept,
            reject,
        } => {
            map(condition);
            map_block(accept);
            map_block(reject);
        }
        Statement::Switch { selector, cases } => {
            map(selector);
            for case in cases {
                map_block(&mut case.body);
            }
        }
        Statement::Loop {
            body,
            continuing,
            break_if,
        } => {
            map_block(body);
            map_block(continuing);
            map_opt(break_if);
        }
        Statement::Break | Statement::Continue | Statement::Kill => {}
        Statement::Return { value } => map_opt(value),
        Statement::ControlBarrier(_) | Statement::MemoryBarrier(_) => {}
        Statement::Store { pointer, value } => {
            map(pointer);
            map(value);
        }
        Statement::ImageStore {
            image,
            coordinate,
            array_index,
            value,
        } => {
            map(image);
            map(coordinate);
            map_opt(array_index);
            map(value);
        }
        Statement::Atomic {
            pointer,
            value,
            result,
            ..
        } => {
            map(pointer);
            map(value);
            map_opt(result);
        }
        Statement::ImageAtomic {
            image,
            coordinate,
            array_index,
            value,
            ..
        } => {
            map(image);
            map(coordinate);
            map_opt(array_index);
            map(value);
        }
        Statement::WorkGroupUniformLoad { pointer, result } => {
            map(pointer);
            map(result);
        }
        Statement::Call {
            arguments, result, ..
        } => {
            for argument in arguments {
                map(argument);
            }
            map_opt(result);
        }
        Statement::RayQuery { query, fun } => {
            map(query);
            match fun {
                naga::RayQueryFunction::Initialize {
                    acceleration_structure,
                    descriptor,
                } => {
                    map(acceleration_structure);
                    map(descriptor);
                }
                naga::RayQueryFunction::Proceed { result } => map(result),
                naga::RayQueryFunction::GenerateIntersection { hit_t } => map(hit_t),
                naga::RayQueryFunction::ConfirmIntersection | naga::RayQueryFunction::Terminate => {
                }
            }
        }
        Statement::RayPipelineFunction(fun) => match fun {
            naga::RayPipelineFunction::TraceRay {
                acceleration_structure,
                descriptor,
                payload,
                ..
            } => {
                map(acceleration_structure);
                map(descriptor);
                map(payload);
            }
        },
        Statement::SubgroupBallot { result, predicate } => {
            map(result);
            map_opt(predicate);
        }
        Statement::SubgroupGather {
            mode,
            argument,
            result,
        } => {
            match mode {
                naga::GatherMode::BroadcastFirst => {}
                naga::GatherMode::Broadcast(expr)
                | naga::GatherMode::Shuffle(expr)
                | naga::GatherMode::ShuffleDown(expr)
                | naga::GatherMode::ShuffleUp(expr)
                | naga::GatherMode::ShuffleXor(expr)
                | naga::GatherMode::QuadBroadcast(expr) => map(expr),
                naga::GatherMode::QuadSwap(_) => {}
            }
            map(argument);
            map(result);
        }
        Statement::SubgroupCollectiveOperation {
            argument, result, ..
        } => {
            map(argument);
            map(result);
        }
        Statement::CooperativeStore { target, data } => {
            map(target);
            map(&mut data.pointer);
            map(&mut data.stride);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_fragment(glsl: &str) -> naga::Module {
        let mut frontend = naga::front::glsl::Frontend::default();
        let options = naga::front::glsl::Options::from(naga::ShaderStage::Fragment);
        frontend.parse(&options, glsl).expect("glsl parses")
    }

    fn validate(module: &naga::Module) -> naga::valid::ModuleInfo {
        let mut validator = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::default(),
        );
        validator.validate(module).expect("module validates")
    }

    fn count_clamps(module: &naga::Module) -> usize {
        let in_function = |f: &naga::Function| {
            f.expressions
                .iter()
                .filter(|(_, e)| {
                    matches!(
                        e,
                        Expression::Math {
                            fun: MathFunction::Clamp,
                            ..
                        }
                    )
                })
                .count()
        };
        module
            .functions
            .iter()
            .map(|(_, f)| in_function(f))
            .sum::<usize>()
            + module
                .entry_points
                .iter()
                .map(|ep| in_function(&ep.function))
                .sum::<usize>()
    }

    #[test]
    fn scalar_tanh_gets_clamped_argument() {
        let mut module = parse_fragment(
            "#version 450 core\n\
             layout(location = 0) out vec4 color;\n\
             void main() { color = vec4(tanh(gl_FragCoord.x)); }\n",
        );
        assert_eq!(count_clamps(&module), 0);
        let rewritten = bound_tanh(&mut module).expect("pass runs");
        assert_eq!(rewritten, 1);
        assert_eq!(count_clamps(&module), 1);
        let info = validate(&module);
        let wgsl =
            naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty())
                .expect("wgsl-out");
        assert!(wgsl.contains("clamp"), "clamp survives to WGSL:\n{wgsl}");
        assert!(wgsl.contains("tanh"), "tanh survives to WGSL:\n{wgsl}");
    }

    #[test]
    fn vector_tanh_bounds_are_splatted() {
        let mut module = parse_fragment(
            "#version 450 core\n\
             layout(location = 0) out vec4 color;\n\
             void main() { vec4 c = vec4(gl_FragCoord.xyxy); color = tanh(c * c); }\n",
        );
        let rewritten = bound_tanh(&mut module).expect("pass runs");
        assert_eq!(rewritten, 1);
        // The module must still validate: WGSL clamp requires matching arg
        // types, so scalar bounds must have been splatted to vec4.
        validate(&module);
        let splats = module
            .functions
            .iter()
            .flat_map(|(_, f)| f.expressions.iter())
            .chain(
                module
                    .entry_points
                    .iter()
                    .flat_map(|ep| ep.function.expressions.iter()),
            )
            .filter(|(_, e)| matches!(e, Expression::Splat { .. }))
            .count();
        assert!(splats >= 2, "expected splatted clamp bounds, got {splats}");
    }

    #[test]
    fn tanh_in_local_functions_is_rewritten_too() {
        let mut module = parse_fragment(
            "#version 450 core\n\
             layout(location = 0) out vec4 color;\n\
             float squash(float x) { return tanh(x); }\n\
             void main() { color = vec4(squash(gl_FragCoord.x)); }\n",
        );
        let rewritten = bound_tanh(&mut module).expect("pass runs");
        assert_eq!(rewritten, 1);
        validate(&module);
        let function_clamps: usize = module
            .functions
            .iter()
            .map(|(_, f)| {
                f.expressions
                    .iter()
                    .filter(|(_, e)| {
                        matches!(
                            e,
                            Expression::Math {
                                fun: MathFunction::Clamp,
                                ..
                            }
                        )
                    })
                    .count()
            })
            .sum();
        assert_eq!(function_clamps, 1, "clamp lands in the local function");
    }

    #[test]
    fn modules_without_tanh_are_untouched() {
        let mut module = parse_fragment(
            "#version 450 core\n\
             layout(location = 0) out vec4 color;\n\
             void main() { color = vec4(sin(gl_FragCoord.x)); }\n",
        );
        let rewritten = bound_tanh(&mut module).expect("pass runs");
        assert_eq!(rewritten, 0);
        assert_eq!(count_clamps(&module), 0);
        validate(&module);
    }
}
