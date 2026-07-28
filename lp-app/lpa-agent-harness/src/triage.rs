//! Bug-class triage over one session's debug dump.
//!
//! The runner writes `triage.txt` from these matches: one line per known
//! compiler-bug-class signature found in the session's engine/tool error
//! text, so a headless run ends with "this looks like the wasm stack-leak
//! class" instead of a raw JSON dump. P2 ships the seam plus the wasm
//! stack-fallthru rule; P3 grows the rule table (rv32lpn builtin-inventory
//! gaps, swizzle-store, ...).

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
    ]
}

/// Scan a dump for known bug-class signatures: every tool-result content
/// block (engine/tool error text rides there verbatim) is matched against
/// [`triage_rules`]. Returns one formatted line per (rule, first matching
/// excerpt) — empty when nothing matched.
pub fn triage_dump(dump: &Dump) -> Vec<String> {
    let mut lines = Vec::new();
    for rule in triage_rules() {
        let excerpt = dump
            .messages
            .iter()
            .flat_map(|message| &message.content)
            .find_map(|block| match block {
                ContentBlock::ToolResult { content, .. } if content.contains(rule.needle) => {
                    Some(excerpt_around(content, rule.needle))
                }
                _ => None,
            });
        if let Some(excerpt) = excerpt {
            lines.push(format!(
                "[{}] {}\n  matched: ...{}...",
                rule.class, rule.verdict, excerpt
            ));
        }
    }
    lines
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
    fn clean_session_triages_to_nothing() {
        let dump = dump_with_tool_result(r#"{"shader":"ok","engine":{"status":"ok"}}"#);
        assert!(triage_dump(&dump).is_empty());
    }
}
