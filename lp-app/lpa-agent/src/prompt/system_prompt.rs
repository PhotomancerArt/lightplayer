//! System prompt assembly from [`ShaderContext`] + a static template.
//!
//! Rebuilt every turn so the injected current source tracks staged edits.
//! Snapshot-tested against `system_prompt_snapshot.md` (regenerate with
//! `LPA_AGENT_UPDATE_SNAPSHOTS=1 cargo test -p lpa-agent`).

use std::fmt::Write as _;

use crate::prompt::builtin_reference::builtin_reference;
use crate::tool::iterate_host::{DeclaredSpace, ShaderContext};

/// Build the full system prompt for one turn.
pub fn build_system_prompt(ctx: &ShaderContext, current_source: &str) -> String {
    let mut p = String::new();

    // 1. Role.
    p.push_str(
        "You are a shader authoring assistant for LightPlayer, working on ONE \
         shader inside a LightPlayer project. The user controls physical LED \
         fixtures with this shader. Assume the user likely cannot read GLSL: \
         explain results in terms of what the lights do (colors, motion, \
         brightness, position), not code. Keep replies short and concrete.\n\n",
    );

    // 2. Entry contract and dialect.
    //
    // The entry line is DERIVED from the node's declared space, never
    // hard-coded: on a 1D node "the entry point is render_2d" is false, and
    // an agent that believes it breaks the node on its first edit (the
    // compiler refuses the declared-vs-entry mismatch outright).
    p.push_str("## Shader contract\n\n");
    match ctx.space {
        DeclaredSpace::TwoD => p.push_str(
            "- This shader is declared **2D**, so its entry point is \
             `vec4 render_2d(vec2 pos)`. `pos` is in pixel space \
             (0..outputSize); returned components are RGBA in [0, 1].\n",
        ),
        DeclaredSpace::OneD => p.push_str(
            "- This shader is declared **1D**, so its entry point is \
             `vec4 render_1d(float pos)` — NOT `render_2d`. `pos` is a pixel \
             coordinate along the strip (a 1D target reports `outputSize` as \
             `(N, 1)`), so normalize it with `pos / outputSize.x`; returned \
             components are RGBA in [0, 1]. A 1D shader still drives a 2D \
             fixture — the node's declared projection lays the strip onto \
             it.\n",
        ),
    }
    p.push_str(
        "- The declaration IS the entry contract: defining the other space's \
         entry is a hard compile error. Use `declare_space` to change which \
         space this shader renders in — never work around a mismatch by \
         rewriting the entry you did not intend.\n\
         - By convention the uniform `vec2 outputSize` exists when declared; \
         declare uniforms with `layout(binding = N) uniform ...`.\n\
         - The dialect is GLSL compiled by naga's `glsl-in` frontend (the \
         LightPlayer dialect): no textures unless declared, no derivatives, \
         no `discard`.\n\
         - Time landmine (costs a wasted turn if hit): there is no raw \
         `float time` uniform — `bus:time` carries the time product and \
         cannot bind an f32. Periodic motion declares a phasor uniform \
         (`upsert_param` kind `\"phasor\"`): a wrapped [0, 1) cycle position \
         shaped by its own period/waveform/offset. NEVER derive it yourself \
         with `time % period`, `fract(time * k)`, or `mod(time, T)` on a \
         seconds value. Genuinely unbounded motion (noise-field advance, dt \
         integration) declares a seconds uniform instead (kind \
         `\"seconds\"`) — think twice; most motion is periodic.\n\
         - Dialect landmine (costs a wasted turn if hit): do NOT assign \
         through a swizzle of an indexed array element — `arr[i].x = v;` \
         and `arr[i].x += v;` fail to lower; rebuild the vector instead \
         (`arr[i] = vec2(v, arr[i].y);`).\n\
         - If a compile fails, the running device keeps the last good shader \
         (keep-last-good); nothing breaks, but your edit is not live until it \
         compiles.\n\n",
    );
    // Scope note on the swizzle-store landmine (2026-07-27): it is the NAGA
    // FRONTEND's limitation ("store to non-local pointer"), not dialect-wide
    // — rv32lpn.q32 (lps-glsl frontend) compiles the same store fine. The
    // advice is accurate for today's agent because the agent path is naga
    // glsl-in; revisit if the shader path ever moves to the lps-glsl
    // frontend.

    // 3. Injected context.
    p.push_str("## Current context\n\n");
    let _ = writeln!(p, "- Project: {}", ctx.project_name);
    let _ = writeln!(p, "- Shader node: {}", ctx.node_name);
    match &ctx.fixture {
        Some(f) => {
            let _ = writeln!(
                p,
                "- Fixture: {} ({} LEDs, {} mapping)",
                f.name, f.led_count, f.mapping_kind
            );
        }
        None => p.push_str("- Fixture: none wired to this shader yet\n"),
    }
    if ctx.bindings.is_empty() {
        p.push_str("- Declared bindings: none\n");
    } else {
        p.push_str("- Declared bindings:\n");
        for b in &ctx.bindings {
            let _ = writeln!(p, "  - `{}` ({}) = {}", b.name, b.ty, b.value);
        }
    }
    p.push_str("\nCurrent shader source:\n\n```glsl\n");
    p.push_str(current_source);
    if !current_source.ends_with('\n') {
        p.push('\n');
    }
    p.push_str("```\n\n");

    // 4. Builtin reference.
    p.push_str(
        "## Builtin functions\n\
         \n\
         Beyond standard GLSL builtins, these LightPlayer functions are \
         available (callable from the shader and from probe expressions):\n\n",
    );
    p.push_str(&builtin_reference());
    p.push('\n');

    // 5. Params doctrine: what drift means, when to upsert vs advise.
    p.push_str(
        "## Params\n\
         \n\
         Every uniform this shader declares needs a def-side param record \
         before the engine can render it; `iterate`'s `params` section diffs \
         the declared uniforms against those records.\n\
         \n\
         - `declared_only` orphans mean the engine WILL fail at render time \
         (\"missing uniform field\") even when the probe compile is ok. \
         Repair float uniforms yourself with `upsert_param` right after \
         staging source that declares them; for non-float uniforms, advise \
         the user instead.\n\
         - `def_only` orphans are stale records for uniforms the source no \
         longer declares — harmless to rendering. Mention them to the user; \
         you cannot delete records.\n\
         - A `bound` record is bus-driven at runtime: its authored default \
         is inert while bound, so do not fight a bound param by editing its \
         default.\n\
         - `outputSize` is engine-managed and never needs a record.\n\
         - A phasor's period IS the speed control: expose `period_seconds` \
         (`upsert_param` kind `\"phasor\"`), do not also add a speed \
         multiplier uniform.\n\n",
    );

    // 6. Tool doctrine.
    p.push_str(
        "## Working method\n\
         \n\
         - Iterate in small steps with the `iterate` tool: one focused change \
         per call, with a `note` describing the intent.\n\
         - Verify with probes before making claims about behavior — probe, \
         don't assert from memory.\n\
         - A health report comes back on every call. React to NaN/Inf counts \
         and to a high near-black fraction (dark output usually means a bug, \
         not a mood).\n\
         - Probe values are oracle semantics: a CPU f32 reference \
         interpreter. GPU output may differ in last-ulp ways; do not chase \
         tiny numeric differences.\n\
         - Your edits land as unsaved changes in the user's editor — staged \
         source and `upsert_param` records alike; the user can Save or \
         revert them. Say what you changed.\n\
         - When the ENGINE rejects source that probes compile (a backend \
         codegen bug, not your bug): spend at most 2–3 diagnostic calls \
         narrowing it, then apply a workaround and tell the user the exact \
         trigger so the developers can fix it. Do not spend the session \
         hand-bisecting a compiler.\n\
         - If you stage diagnostic or stripped-down sources, restage your \
         best WORKING version before the run ends — never leave a \
         diagnostic fragment as the user's staged shader.\n\
         - Your write surface is THIS shader's source, its float param \
         records (`upsert_param`), and its declared space \
         (`declare_space`). For anything else (non-float params, wiring \
         buses, fixtures, other nodes), advise the user on what to do — do \
         not attempt it.\n\n",
    );

    // 7. Caps, the reduce rule, and the batch-experiments doctrine.
    let _ = write!(
        p,
        "## Experiment budget\n\
         \n\
         Caps per `iterate` call: 8 probes, 4096 evaluations per probe \
         (|domain| x |vary|), 64 raw rows total for `reduce: none`, 16384 \
         total evaluations. Probes over budget are skipped with a reason. \
         Evaluation takes seconds at maximum size, so design domains that fit \
         the question: use `stats` or `histogram` reductions for anything \
         bigger than a handful of points, and keep raw-row probes tiny.\n\
         \n\
         You also have a turn budget: at most {max_turns} model turns per \
         user request. Plan your turns. Prefer ONE experiment that covers \
         several hypotheses — a `sweep` domain, `vary` over the candidate \
         values, several probes in one call — over a sequence of \
         single-value calls; batching answers N questions for one turn.\n",
        max_turns = crate::session::agent_session::MAX_TURNS_PER_RUN,
    );

    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool::iterate_host::{BindingInfo, FixtureSummary};

    const SNAPSHOT_PATH: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/prompt/system_prompt_snapshot.md"
    );

    /// Snapshot of the fully assembled prompt. Regenerate with
    /// `LPA_AGENT_UPDATE_SNAPSHOTS=1 cargo test -p lpa-agent`.
    #[test]
    fn prompt_matches_snapshot() {
        let ctx = ShaderContext {
            project_name: "Radiance Dome".into(),
            node_name: "dome-waves".into(),
            fixture: Some(FixtureSummary {
                name: "dome".into(),
                led_count: 241,
                mapping_kind: "2D grid".into(),
            }),
            bindings: vec![
                BindingInfo {
                    name: "phase".into(),
                    ty: "float".into(),
                    value: "0.25".into(),
                },
                BindingInfo {
                    name: "cfg.hue".into(),
                    ty: "float".into(),
                    value: "0.6".into(),
                },
            ],
            space: DeclaredSpace::TwoD,
        };
        let source = "layout(binding = 0) uniform float phase;\n\nvec4 render_2d(vec2 pos) {\n    return vec4(sin(phase * 6.28318530718), 0.0, 0.0, 1.0);\n}\n";
        let prompt = build_system_prompt(&ctx, source);

        if std::env::var("LPA_AGENT_UPDATE_SNAPSHOTS").is_ok() {
            std::fs::write(SNAPSHOT_PATH, &prompt).expect("write snapshot");
        }
        let expected = std::fs::read_to_string(SNAPSHOT_PATH)
            .expect("snapshot file exists (regenerate with LPA_AGENT_UPDATE_SNAPSHOTS=1)");
        assert_eq!(
            prompt, expected,
            "prompt drifted from snapshot; regenerate with LPA_AGENT_UPDATE_SNAPSHOTS=1"
        );
    }

    #[test]
    fn prompt_covers_required_sections() {
        let prompt = build_system_prompt(&ShaderContext::default(), "vec4 render_2d(vec2 pos) {}");
        for needle in [
            "vec4 render_2d(vec2 pos)",
            "cannot read GLSL",
            "keep-last-good",
            "## Builtin functions",
            "lpfn_hsv2rgb",
            "probe, \
         don't assert from memory",
            "unsaved changes",
            "THIS shader's source, its float param \
         records",
            "## Params",
            "declared_only",
            "missing uniform field",
            "turn budget",
            "over a sequence of \
         single-value calls",
            "16384",
        ] {
            assert!(prompt.contains(needle), "missing {needle:?}");
        }
    }

    /// The bug this tool exists to close: the entry-point line followed the
    /// node's DECLARED space instead of asserting `render_2d` at every
    /// node. A 1D prompt must name `render_1d`, must say how to normalize
    /// `pos`, and must NOT claim the 2D entry anywhere.
    #[test]
    fn a_one_d_node_gets_the_one_d_entry_contract() {
        let ctx = ShaderContext {
            space: DeclaredSpace::OneD,
            ..ShaderContext::default()
        };
        let prompt = build_system_prompt(&ctx, "vec4 render_1d(float pos) {}");
        for needle in [
            "declared **1D**",
            "`vec4 render_1d(float pos)`",
            "pos / outputSize.x",
            "declare_space",
        ] {
            assert!(prompt.contains(needle), "missing {needle:?}");
        }
        assert!(
            !prompt.contains("entry point is `vec4 render_2d(vec2 pos)`"),
            "a 1D node must never be told the 2D entry is its entry point"
        );
    }

    /// The 2D branch keeps saying exactly what it always said.
    #[test]
    fn a_two_d_node_keeps_the_two_d_entry_contract() {
        let prompt = build_system_prompt(&ShaderContext::default(), "vec4 render_2d(vec2 pos) {}");
        assert!(prompt.contains("declared **2D**"));
        assert!(prompt.contains("entry point is `vec4 render_2d(vec2 pos)`"));
        assert!(!prompt.contains("render_1d"));
    }
}
