//! The `declare_space` tool: author the shader node's declared
//! dimensionality (`ShaderDef::space`).
//!
//! Its own tool, not an `iterate` argument — a write op deserves an
//! explicit call (see the studio-shader-agent ADR). Without it the agent
//! can stage a `render_1d` body onto a `TwoD`-declared node, watch the
//! compiler refuse the mismatch in its engine verdict, and have no way to
//! fix what it just broke; the user had to open the dimensionality drawer
//! by hand.
//!
//! Execution order mirrors [`crate::tool::upsert_param_tool`]: validate
//! the input as DATA (an actionable in-band error; `is_error` stays
//! reserved for host failures) → dispatch through the host's Save-gated
//! overlay path → bounded engine-verdict wait (the declaration IS the
//! entry contract, so a change flips the node's compile state) → report
//! `{applied, space, engine}`.
//!
//! There is no pre-check against the current source. Declaring the space
//! the GLSL does not yet match is the NORMAL repair order in both
//! directions — declare 1D then write `render_1d`, or the reverse — so
//! gating the write on a matching entry point would refuse exactly the
//! call that fixes a mismatch.

use serde::Deserialize;
use serde_json::{Value, json};

use crate::provider::model_provider::ToolDef;
use crate::tool::iterate_host::{AgentHost, DeclaredSpace, ProjectionShapeTag, SpaceDeclaration};
use crate::tool::iterate_tool::{ENGINE_VERDICT_BUDGET_MS, IterateOutcome, yield_to_ui};
use crate::tool::tool_phase::ToolPhase;

/// The tool's wire name.
pub const DECLARE_SPACE_TOOL_NAME: &str = "declare_space";

/// Parsed `declare_space` input. Unknown fields are rejected so the model
/// gets an actionable error instead of a silently ignored argument.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclareSpaceInput {
    pub space: String,
    #[serde(default)]
    pub shape: Option<String>,
    #[serde(default)]
    pub mirror: Option<bool>,
    #[serde(default)]
    pub flip: Option<bool>,
}

/// Execute one `declare_space` call. Same outcome/phase contract as
/// [`crate::tool::iterate_tool::run_iterate`].
pub async fn run_declare_space(
    input_json: &Value,
    host: &mut dyn AgentHost,
    progress: &mut dyn FnMut(ToolPhase),
) -> IterateOutcome {
    let input: DeclareSpaceInput = match serde_json::from_value(input_json.clone()) {
        Ok(input) => input,
        Err(e) => {
            return data_outcome(
                json!({ "input_error": format!("invalid declare_space input: {e}") }),
                json!({ "input_error": true }),
            );
        }
    };

    let Some(space) = DeclaredSpace::parse(&input.space) else {
        return data_outcome(
            json!({ "error": format!(
                "`{}` is not a valid space — use \"1d\" or \"2d\"",
                input.space
            ) }),
            json!({ "note": "space", "error": "invalid space" }),
        );
    };

    // The projection fields describe how a 1D source answers 2D consumers;
    // on a 2D declaration there is no such record to write. Refuse rather
    // than drop them silently (the `deny_unknown_fields` posture).
    let projection_fields = input.shape.is_some() || input.mirror.is_some() || input.flip.is_some();
    if space == DeclaredSpace::TwoD && projection_fields {
        return data_outcome(
            json!({ "error":
                "`shape`/`mirror`/`flip` describe how a 1D shader is projected onto a 2D \
                 consumer — they require `space: \"1d\"`. A 2D shader is sampled directly; \
                 its only answer to 1D consumers is the centre scanline, which is not \
                 authorable."
            }),
            json!({ "note": "space `2d`", "error": "projection fields on a 2d declaration" }),
        );
    }

    let shape = match input.shape.as_deref().map(ProjectionShapeTag::parse) {
        Some(None) => {
            let tags = ProjectionShapeTag::TAGS;
            return data_outcome(
                json!({ "error": format!(
                    "`{}` is not a valid projection shape — use one of {tags:?}",
                    input.shape.unwrap_or_default()
                ) }),
                json!({ "note": "space `1d`", "error": "invalid shape" }),
            );
        }
        Some(Some(shape)) => Some(shape),
        None => None,
    };

    let declaration = SpaceDeclaration {
        space,
        shape,
        mirror: input.mirror,
        flip: input.flip,
    };

    progress(ToolPhase::Staging);
    yield_to_ui().await;
    if let Err(e) = host.declare_space(&declaration).await {
        return host_error(format!("space declaration failed: {}", e.message));
    }

    // The declaration IS the entry contract, so the node recompiles: the
    // SAME verdict window as a staged source edit reports the outcome —
    // including the mismatch this call was probably made to fix.
    progress(ToolPhase::AwaitingEngine);
    yield_to_ui().await;
    let engine = host.await_engine_verdict(ENGINE_VERDICT_BUDGET_MS).await;

    progress(ToolPhase::Finishing);
    yield_to_ui().await;

    let mut content = serde_json::Map::new();
    content.insert("applied".into(), json!(true));
    content.insert("space".into(), declaration_echo(&declaration));
    content.insert("entry_point".into(), json!(entry_point(space)));
    let mut summary = json!({
        "note": format!("space `{}`", space.tag()),
        "applied": true,
        "staged": true,
    });
    if let Some(verdict) = &engine {
        let engine = crate::tool::iterate_tool::engine_json(verdict);
        content.insert("engine".into(), engine.clone());
        summary
            .as_object_mut()
            .expect("summary is an object")
            .insert("engine".into(), engine);
    }
    IterateOutcome {
        content: Value::Object(content).to_string(),
        is_error: false,
        summary,
    }
}

/// The entry point a declared space contracts for — echoed on every write
/// so the model is reminded what its next `iterate` has to define.
pub fn entry_point(space: DeclaredSpace) -> &'static str {
    match space {
        DeclaredSpace::TwoD => "vec4 render_2d(vec2 pos)",
        DeclaredSpace::OneD => "vec4 render_1d(float pos)",
    }
}

/// Echo the written fields, so the transcript records what landed.
fn declaration_echo(declaration: &SpaceDeclaration) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("space".into(), json!(declaration.space.tag()));
    if let Some(shape) = declaration.shape {
        obj.insert("shape".into(), json!(shape.tag()));
    }
    if let Some(mirror) = declaration.mirror {
        obj.insert("mirror".into(), json!(mirror));
    }
    if let Some(flip) = declaration.flip {
        obj.insert("flip".into(), json!(flip));
    }
    Value::Object(obj)
}

/// The tool definition offered to the model every turn.
pub fn declare_space_tool_def() -> ToolDef {
    ToolDef {
        name: DECLARE_SPACE_TOOL_NAME.into(),
        description: DESCRIPTION.into(),
        input_schema: input_schema(),
    }
}

const DESCRIPTION: &str = "\
Declare which space this shader renders in. The declaration IS the entry \
contract the compiler enforces: a `2d` shader must define \
`vec4 render_2d(vec2 pos)`, a `1d` shader must define \
`vec4 render_1d(float pos)` — a mismatch is a hard compile error \
(\"declared 1D but defines `render_2d`\"). Call this BEFORE staging source \
with the other entry point, or right after the engine reports a mismatch. \
Prefer `1d` for patterns that are genuinely a line of pixels (comets, fire, \
palette sweeps): a 1D shader still drives a 2D fixture through its \
projection. For `1d` you may also set the projection 2D consumers see — \
`shape` (the base coordinate map), plus `mirror` (fold the strip around the \
map's midpoint) and `flip` (reverse it, applied after the fold); omitted \
fields keep their current value, and a fresh declaration starts at plain \
extrude-x. Those three are rejected on a `2d` declaration. The edit lands \
as an unsaved change exactly like staged source — the user Saves or reverts \
it. Returns the engine's post-declaration verdict.";

fn input_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["space"],
        "properties": {
            "space": { "enum": ["1d", "2d"],
                "description": "The space this shader renders in. \"2d\" contracts for `vec4 render_2d(vec2 pos)` (pos in pixel space, 0..outputSize); \"1d\" contracts for `vec4 render_1d(float pos)` (pos along the strip, normalized with `pos / outputSize.x`)." },
            "shape": { "enum": ProjectionShapeTag::TAGS,
                "description": "How a 1D shader is laid onto a 2D consumer: \"extrude-x\" runs the strip along the columns (the default), \"extrude-y\" along the rows, \"radial\" out from the centre, \"angular\" around it. Requires space \"1d\"." },
            "mirror": { "type": "boolean",
                "description": "Fold the strip around the map's midpoint, so it runs out and back. Requires space \"1d\"." },
            "flip": { "type": "boolean",
                "description": "Reverse the strip, applied after the fold. Requires space \"1d\"." }
        }
    })
}

fn data_outcome(content: Value, summary: Value) -> IterateOutcome {
    IterateOutcome {
        content: content.to_string(),
        is_error: false,
        summary,
    }
}

fn host_error(message: String) -> IterateOutcome {
    IterateOutcome {
        content: json!({ "error": &message }).to_string(),
        is_error: true,
        summary: json!({ "error": message }),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use lps_probe::LedPoint;

    use super::*;
    use crate::tool::iterate_host::{
        EngineStatusKind, EngineVerdict, HostError, HostFuture, ShaderContext,
    };

    fn run(input: &Value, host: &mut FakeHost) -> IterateOutcome {
        futures_executor::block_on(run_declare_space(input, host, &mut |_| {}))
    }

    #[test]
    fn schema_requires_space_and_rejects_unknown_fields() {
        let def = declare_space_tool_def();
        assert_eq!(def.name, "declare_space");
        assert_eq!(def.input_schema["required"], json!(["space"]));
        assert_eq!(def.input_schema["additionalProperties"], false);
        assert_eq!(
            def.input_schema["properties"]["space"]["enum"],
            json!(["1d", "2d"])
        );

        let input: DeclareSpaceInput = serde_json::from_value(json!({
            "space": "1d", "shape": "radial", "mirror": true, "flip": false,
        }))
        .expect("full input parses");
        assert_eq!(input.space, "1d");
        assert_eq!(input.shape.as_deref(), Some("radial"));
        assert_eq!(input.mirror, Some(true));

        let sparse: DeclareSpaceInput =
            serde_json::from_value(json!({ "space": "2d" })).expect("space-only input parses");
        assert_eq!(sparse.shape, None);

        serde_json::from_value::<DeclareSpaceInput>(json!({ "space": "1d", "shpae": "radial" }))
            .expect_err("unknown fields are rejected");
        serde_json::from_value::<DeclareSpaceInput>(json!({ "shape": "radial" }))
            .expect_err("space is required");
    }

    #[test]
    fn a_one_d_declaration_dispatches_and_reports_the_engine_verdict() {
        let mut host = FakeHost::default();
        host.verdict = Some(EngineVerdict {
            status: EngineStatusKind::Ok,
            message: None,
            line_col: None,
        });
        let mut phases = Vec::new();
        let outcome = futures_executor::block_on(run_declare_space(
            &json!({ "space": "1d", "shape": "radial", "mirror": true }),
            &mut host,
            &mut |phase| phases.push(phase),
        ));
        assert!(!outcome.is_error);
        let content: Value = serde_json::from_str(&outcome.content).expect("json");
        assert_eq!(content["applied"], true);
        assert_eq!(content["space"]["space"], "1d");
        assert_eq!(content["space"]["shape"], "radial");
        assert_eq!(content["space"]["mirror"], true);
        assert!(
            content["space"].get("flip").is_none(),
            "unset fields omitted"
        );
        assert_eq!(
            content["entry_point"], "vec4 render_1d(float pos)",
            "the write echoes the entry contract it just set"
        );
        assert_eq!(content["engine"]["status"], "ok");
        assert_eq!(outcome.summary["note"], "space `1d`");
        assert_eq!(outcome.summary["staged"], true);

        assert_eq!(
            host.declarations.borrow().as_slice(),
            [SpaceDeclaration {
                space: DeclaredSpace::OneD,
                shape: Some(ProjectionShapeTag::Radial),
                mirror: Some(true),
                flip: None,
            }]
        );
        assert_eq!(
            host.verdict_budgets.borrow().as_slice(),
            [ENGINE_VERDICT_BUDGET_MS]
        );
        // No Compiling phase: unlike `upsert_param` there is nothing to
        // pre-check the declaration against.
        assert_eq!(
            phases,
            vec![
                ToolPhase::Staging,
                ToolPhase::AwaitingEngine,
                ToolPhase::Finishing,
            ]
        );
    }

    #[test]
    fn a_bare_two_d_declaration_writes_only_the_space() {
        let mut host = FakeHost::default();
        let outcome = run(&json!({ "space": "2d" }), &mut host);
        assert!(!outcome.is_error);
        let content: Value = serde_json::from_str(&outcome.content).expect("json");
        assert_eq!(content["space"]["space"], "2d");
        assert_eq!(content["entry_point"], "vec4 render_2d(vec2 pos)");
        assert_eq!(
            host.declarations.borrow().as_slice(),
            [SpaceDeclaration {
                space: DeclaredSpace::TwoD,
                ..SpaceDeclaration::default()
            }]
        );
    }

    #[test]
    fn an_invalid_space_is_an_actionable_in_band_error() {
        let mut host = FakeHost::default();
        let outcome = run(&json!({ "space": "3d" }), &mut host);
        assert!(!outcome.is_error, "bad input is data");
        let content: Value = serde_json::from_str(&outcome.content).expect("json");
        let error = content["error"].as_str().expect("error");
        assert!(error.contains("`3d`"), "{error}");
        assert!(host.declarations.borrow().is_empty(), "nothing dispatched");
    }

    #[test]
    fn an_invalid_shape_is_an_actionable_in_band_error() {
        let mut host = FakeHost::default();
        let outcome = run(&json!({ "space": "1d", "shape": "spiral" }), &mut host);
        assert!(!outcome.is_error);
        let content: Value = serde_json::from_str(&outcome.content).expect("json");
        let error = content["error"].as_str().expect("error");
        assert!(error.contains("`spiral`"), "{error}");
        assert!(error.contains("extrude-x"), "valid tags listed: {error}");
        assert!(host.declarations.borrow().is_empty(), "nothing dispatched");
    }

    /// The task's explicit posture: reject the combination rather than
    /// silently ignoring the projection fields.
    #[test]
    fn projection_fields_on_a_two_d_declaration_are_refused() {
        for extra in [
            json!({ "space": "2d", "shape": "radial" }),
            json!({ "space": "2d", "mirror": true }),
            json!({ "space": "2d", "flip": true }),
        ] {
            let mut host = FakeHost::default();
            let outcome = run(&extra, &mut host);
            assert!(!outcome.is_error);
            let content: Value = serde_json::from_str(&outcome.content).expect("json");
            let error = content["error"].as_str().expect("error");
            assert!(error.contains("space: \"1d\""), "{error}");
            assert!(
                host.declarations.borrow().is_empty(),
                "nothing dispatched for {extra}"
            );
        }
    }

    #[test]
    fn host_refusal_is_an_error_result() {
        let mut host = FakeHost {
            fail: true,
            ..FakeHost::default()
        };
        let outcome = run(&json!({ "space": "1d" }), &mut host);
        assert!(outcome.is_error, "host failures are the only is_error case");
        assert!(outcome.content.contains("space declaration failed"));
    }

    #[test]
    fn entry_points_match_the_compiler_contract() {
        assert_eq!(entry_point(DeclaredSpace::TwoD), "vec4 render_2d(vec2 pos)");
        assert_eq!(
            entry_point(DeclaredSpace::OneD),
            "vec4 render_1d(float pos)"
        );
    }

    // -- helpers ----------------------------------------------------------

    #[derive(Default)]
    struct FakeHost {
        declarations: RefCell<Vec<SpaceDeclaration>>,
        fail: bool,
        verdict: Option<EngineVerdict>,
        verdict_budgets: RefCell<Vec<u32>>,
    }

    impl AgentHost for FakeHost {
        fn current_source(&self) -> Result<String, HostError> {
            Ok(String::new())
        }

        fn stage_source<'a>(
            &'a mut self,
            _source: &'a str,
        ) -> HostFuture<'a, Result<(), HostError>> {
            Box::pin(async { Ok(()) })
        }

        fn declare_space<'a>(
            &'a mut self,
            declaration: &'a SpaceDeclaration,
        ) -> HostFuture<'a, Result<(), HostError>> {
            Box::pin(async move {
                if self.fail {
                    return Err(HostError::new("overlay write refused"));
                }
                self.declarations.borrow_mut().push(declaration.clone());
                Ok(())
            })
        }

        fn await_engine_verdict(
            &mut self,
            budget_ms: u32,
        ) -> HostFuture<'_, Option<EngineVerdict>> {
            self.verdict_budgets.borrow_mut().push(budget_ms);
            let verdict = self.verdict.clone();
            Box::pin(async move { verdict })
        }

        fn led_points(&self) -> Vec<LedPoint> {
            Vec::new()
        }

        fn shader_context(&self) -> ShaderContext {
            ShaderContext::default()
        }
    }

    /// The default host seam refuses the write with an actionable message
    /// (evals and test stubs never edit the project).
    #[test]
    fn the_default_host_cannot_declare_space() {
        struct Bare;
        impl AgentHost for Bare {
            fn current_source(&self) -> Result<String, HostError> {
                Ok(String::new())
            }
            fn stage_source<'a>(
                &'a mut self,
                _source: &'a str,
            ) -> HostFuture<'a, Result<(), HostError>> {
                Box::pin(async { Ok(()) })
            }
            fn led_points(&self) -> Vec<LedPoint> {
                Vec::new()
            }
            fn shader_context(&self) -> ShaderContext {
                ShaderContext::default()
            }
        }
        let outcome = futures_executor::block_on(run_declare_space(
            &json!({ "space": "1d" }),
            &mut Bare,
            &mut |_| {},
        ));
        assert!(outcome.is_error);
        assert!(
            outcome
                .content
                .contains("cannot edit the space declaration")
        );
    }
}
