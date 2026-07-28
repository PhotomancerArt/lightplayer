//! Bug-class triage over one session's debug dump.
//!
//! The runner writes `triage.txt` from these matches: one line per known
//! compiler-bug-class signature found in the session's engine/tool error
//! text, so a headless run ends with "this looks like the wasm stack-leak
//! class" instead of a raw JSON dump. Beyond the known signatures,
//! [`triage_dump`] flags the INTERESTING case: an engine error on a source
//! the probe world accepted that matches no known class — that split is
//! how every compiler bug this round surfaced, so an unmatched one is a
//! "NEW class — investigate" line, not silence.

use lpa_agent::ContentBlock;

use crate::dump::Dump;

/// One bug-class signature: a substring match over error text plus the
/// classification line the triage report prints.
pub struct TriageRule {
    /// Substring looked for in engine/tool error messages.
    pub needle: &'static str,
    /// Short class name (one token, stable for grepping).
    pub class: &'static str,
    /// What the match means and where the class is documented.
    pub verdict: &'static str,
}

/// The known bug-class signatures, first match per error wins.
pub fn triage_rules() -> &'static [TriageRule] {
    &[
        // The Q32 inline-emit stack-imbalance class (fabs leak, #155):
        // wasm only checks stack balance at reachable block ends, so the
        // leak surfaces inside loops/ifs and masquerades as broken control
        // flow. Suspect an EXPRESSION op's emission before believing the
        // control flow is at fault.
        TriageRule {
            needle: "elements on the stack for fallthru",
            class: "wasm-stack-leak",
            verdict: "Q32 inline-emit stack imbalance (lpvm-wasm emit leaks a value; \
                      see docs/defects/2026-07-27-wasm-q32-fabs-stack-leak.md — check \
                      expression ops inside if/loop blocks, not the control flow)",
        },
        // The lps-glsl builtin-inventory gap class (step(), 2026-07-27):
        // typeck's callable inventory lags the builtins registry, so a
        // perfectly ordinary builtin call is refused by name. Batch-triage:
        // one gap usually means siblings (step/smoothstep pattern) — diff
        // lps-glsl typeck's inventory against lps-builtins before fixing
        // just the reported name.
        TriageRule {
            needle: "unsupported call `",
            class: "builtin-inventory-gap",
            verdict: "lps-glsl frontend builtin-inventory gap (typeck refuses a builtin by \
                      name; step()-class, 2026-07-27) — batch-triage: diff the typeck \
                      inventory against lps-builtins, siblings are usually missing too",
        },
        // The naga swizzle-store landmine (`arr[i].x = v;` fails to lower;
        // naga-frontend-only). The agent system prompt explicitly warns
        // against this construct, so the signature appearing in a session
        // means the PROMPT DOCTRINE FAILED — fix the prompt wording (or the
        // model's adherence), don't just repair the shader.
        TriageRule {
            needle: "store to non-local pointer",
            class: "naga-swizzle-store",
            verdict: "naga swizzle-store landmine (naga-frontend-only; lps-glsl compiles the \
                      same store) — the system prompt warns against `arr[i].x = v`, so this \
                      appearing means the prompt doctrine FAILED; review the prompt warning, \
                      then rebuild the vector instead",
        },
        // The harness's own over-claim class (first real-provider smoke,
        // 2026-07-28): the runner header said "browser-parity CONFIRMED"
        // while the engine lacked the naga feature entirely. The runner now
        // refuses to start without the frontend built in; this rule is the
        // backstop if the message ever reaches a dump anyway.
        TriageRule {
            needle: "was not built into this binary",
            class: "frontend-missing",
            verdict: "the binary was built WITHOUT the requested GLSL frontend — feature-wiring \
                      regression (lpa-studio-core `harness` must enable lpa-server/naga); every \
                      engine verdict in this run is about the missing feature, not the shader",
        },
    ]
}

/// The line prefix for the unmatched probe-ok/engine-error case (tests and
/// the battery grep for it).
pub const NEW_CLASS_PREFIX: &str = "[NEW]";

/// Scan a dump for known bug-class signatures: every tool-result content
/// block (engine/tool error text rides there verbatim) is matched against
/// [`triage_rules`]. Additionally, any tool result where the PROBE compile
/// succeeded but the ENGINE rejected the source — and no rule matches that
/// result — earns a "NEW class — investigate" line: the probe/engine split
/// is exactly where compiler bugs live, so an unrecognized one is the
/// interesting outcome, not noise. Returns one formatted line per finding —
/// empty when nothing matched.
pub fn triage_dump(dump: &Dump) -> Vec<String> {
    let mut lines = Vec::new();
    for rule in triage_rules() {
        let excerpt = tool_results(dump).find_map(|content| {
            content
                .contains(rule.needle)
                .then(|| excerpt_around(content, rule.needle))
        });
        if let Some(excerpt) = excerpt {
            lines.push(format!(
                "[{}] {}\n  matched: ...{}...",
                rule.class, rule.verdict, excerpt
            ));
        }
    }
    // The fallback: probe-ok/engine-error results no rule explains.
    for content in tool_results(dump) {
        if let Some(message) = unexplained_engine_rejection(content) {
            lines.push(format!(
                "{NEW_CLASS_PREFIX} engine rejected a source the probe world accepted, and no \
                 known bug-class signature matches — investigate (this split is how compiler \
                 bugs surface)\n  engine said: {message}"
            ));
        }
    }
    lines
}

/// Every tool-result content string in transcript order.
fn tool_results(dump: &Dump) -> impl Iterator<Item = &str> {
    dump.messages
        .iter()
        .flat_map(|message| &message.content)
        .filter_map(|block| match block {
            ContentBlock::ToolResult { content, .. } => Some(content.as_str()),
            _ => None,
        })
}

/// If `content` is an iterate result whose probe compile succeeded but
/// whose engine verdict is an error that no [`triage_rules`] needle
/// explains, the engine's message. `None` otherwise.
fn unexplained_engine_rejection(content: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(content).ok()?;
    if value.get("shader")? != "ok" {
        return None;
    }
    let engine = value.get("engine")?;
    if engine.get("status")? != "error" {
        return None;
    }
    if triage_rules()
        .iter()
        .any(|rule| content.contains(rule.needle))
    {
        return None;
    }
    let message = engine
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("(no message)");
    Some(message.to_string())
}

/// A short context window around the first match of `needle` in `text`.
fn excerpt_around(text: &str, needle: &str) -> String {
    let at = text.find(needle).unwrap_or(0);
    let start = at.saturating_sub(60);
    let end = (at + needle.len() + 60).min(text.len());
    // Snap to char boundaries (error text may carry multi-byte chars).
    let start = (0..=start)
        .rev()
        .find(|&i| text.is_char_boundary(i))
        .unwrap_or(0);
    let end = (end..=text.len())
        .find(|&i| text.is_char_boundary(i))
        .unwrap_or(text.len());
    text[start..end].replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::dump::parse_dump;

    fn dump_with_tool_result(content: &str) -> Dump {
        parse_dump(
            &json!({
                "format": 1,
                "artifact": "shader.glsl",
                "provider": "anthropic",
                "model": "m",
                "usage_total": { "input_tokens": 0, "output_tokens": 0 },
                "turns": [],
                "edits": [],
                "messages": [
                    { "role": "user", "content": [
                        { "type": "tool_result", "tool_use_id": "tu_1", "content": content },
                    ] },
                ],
            })
            .to_string(),
        )
        .expect("dump parses")
    }

    #[test]
    fn wasm_stack_fallthru_signature_matches_with_excerpt() {
        let dump = dump_with_tool_result(
            r#"{"engine":{"status":"error","message":"shader WASM parse/validate failed: expected 0 elements on the stack for fallthru, found 2"}}"#,
        );
        let lines = triage_dump(&dump);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].starts_with("[wasm-stack-leak]"), "{}", lines[0]);
        assert!(lines[0].contains("fabs-stack-leak"), "{}", lines[0]);
        assert!(
            lines[0].contains("elements on the stack for fallthru"),
            "{}",
            lines[0]
        );
    }

    #[test]
    fn unsupported_call_signature_flags_the_builtin_inventory_gap() {
        let dump = dump_with_tool_result(
            r#"{"shader":{"err":{"diagnostics":[{"message":"unsupported call `smoothstep`"}]}},"staged":true}"#,
        );
        let lines = triage_dump(&dump);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(
            lines[0].starts_with("[builtin-inventory-gap]"),
            "{}",
            lines[0]
        );
        assert!(lines[0].contains("batch-triage"), "{}", lines[0]);
        assert!(lines[0].contains("smoothstep"), "excerpt: {}", lines[0]);
    }

    #[test]
    fn swizzle_store_signature_flags_prompt_doctrine_failure() {
        let dump = dump_with_tool_result(
            r#"{"shader":"ok","staged":true,"engine":{"status":"error","message":"Lower error: store to non-local pointer"}}"#,
        );
        let lines = triage_dump(&dump);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].starts_with("[naga-swizzle-store]"), "{}", lines[0]);
        assert!(lines[0].contains("prompt doctrine FAILED"), "{}", lines[0]);
        // Probe-ok/engine-error matched by a rule must NOT double as NEW.
        assert!(
            !lines.iter().any(|l| l.starts_with(NEW_CLASS_PREFIX)),
            "{lines:?}"
        );
    }

    #[test]
    fn frontend_missing_signature_flags_the_feature_wiring_regression() {
        let dump = dump_with_tool_result(
            r#"{"shader":"ok","staged":true,"engine":{"status":"error","message":"naga frontend was not built into this binary"}}"#,
        );
        let lines = triage_dump(&dump);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].starts_with("[frontend-missing]"), "{}", lines[0]);
        assert!(lines[0].contains("lpa-server/naga"), "{}", lines[0]);
    }

    #[test]
    fn probe_ok_engine_error_matching_nothing_is_flagged_as_a_new_class() {
        let dump = dump_with_tool_result(
            r#"{"shader":"ok","staged":true,"engine":{"status":"error","message":"missing uniform field `speed`"}}"#,
        );
        let lines = triage_dump(&dump);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].starts_with(NEW_CLASS_PREFIX), "{}", lines[0]);
        assert!(lines[0].contains("investigate"), "{}", lines[0]);
        assert!(
            lines[0].contains("missing uniform field `speed`"),
            "engine message carried: {}",
            lines[0]
        );
    }

    #[test]
    fn probe_failure_with_engine_error_is_not_a_new_class() {
        // Both worlds rejecting the source is an ordinary compile error,
        // not a split — no NEW line.
        let dump = dump_with_tool_result(
            r#"{"shader":{"err":{"diagnostics":[]}},"staged":true,"engine":{"status":"error","message":"compile failed"}}"#,
        );
        assert!(triage_dump(&dump).is_empty());
    }

    #[test]
    fn clean_session_triages_to_nothing() {
        let dump = dump_with_tool_result(r#"{"shader":"ok","engine":{"status":"ok"}}"#);
        assert!(triage_dump(&dump).is_empty());
    }
}
