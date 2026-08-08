//! Format 6 → 7: shader GLSL entries are dimension-explicit.
//!
//! The break (`40b936fbe`, "explicit render entries — render_1d(float) /
//! render_2d(vec2), declaration-driven", dimensionality plan D19/Q11):
//! `vec4 render(vec2 pos)` stopped being a recognized shader entry. A shader
//! now declares a space (`TwoD` or `OneD`, `lp-shader/lp-shader/src/
//! entry_space.rs`) and must define the matching entry — `render_2d` for
//! `TwoD`, `render_1d` for `OneD`. A bare `render` is a hard compile error.
//! No version-6 project could author `OneD` through a released build (the
//! declaration shipped with this bump), so every pre-v7 GLSL asset defines
//! the 2D entry under the old name. (v6 itself was the uid re-render —
//! this step renumbered from v5→v6 when that one merged first.)
//!
//! ## What this step does
//!
//! Rewrites the entry function **definition** in every `.glsl` asset:
//! `vec4 render(` → `vec4 render_2d(`. Behavior-preserving — `render_2d`
//! under a default `TwoD` declaration runs the identical program; only the
//! name changes. Non-shader `.glsl` assets (compute shaders, whose entry is
//! `void tick()`) contain no such signature and pass through untouched.
//!
//! The rewrite is signature-anchored, not a parser: it looks for the literal
//! token sequence `vec4` · whitespace · `render` · optional whitespace ·
//! `(`, as a whole word in both directions. A comment mentioning the word
//! `render` — `// define helpers before render().` — never carries that
//! exact four-token shape immediately adjacent, so it survives untouched;
//! the corpus fixture `basic` (pulled from `examples/basic/shader.glsl`,
//! which has exactly such a comment right above its entry) proves it.
//!
//! `project.json`'s own `format` field is bumped `6` → `7` the same way
//! `v4_to_v5` bumps its manifest; no other JSON content changes; P1's new
//! space-declaration slots are additive defaults, not migrated content.

use crate::json::JsonNode;
use crate::json_file_edit::edit_json_files;
use crate::project_files::{ProjectFiles, is_manifest_path};
use crate::upgrade_error::UpgradeError;
use crate::upgrade_report::UpgradeReport;

const FROM: u32 = 6;
const TO: u32 = 7;

pub(crate) fn apply(
    files: &mut ProjectFiles,
    report: &mut UpgradeReport,
) -> Result<(), UpgradeError> {
    edit_json_files(files, report, |path, document, report| {
        if is_manifest_path(path) {
            bump_manifest_format(path, document, report);
        }
        Ok(())
    })?;
    rewrite_glsl_entries(files, report)?;
    Ok(())
}

/// R1: the manifest's own version stamp, `5` → `6`.
fn bump_manifest_format(path: &str, document: &mut JsonNode, report: &mut UpgradeReport) {
    if document.get("format").and_then(JsonNode::as_u32) == Some(FROM) {
        document.set("format", JsonNode::u32(TO));
        report.note(format!("{path}: format {FROM} → {TO}"));
    }
}

/// Rewrite the `render` entry definition in every `.glsl` asset, only
/// rewriting files that actually changed.
fn rewrite_glsl_entries(
    files: &mut ProjectFiles,
    report: &mut UpgradeReport,
) -> Result<(), UpgradeError> {
    let paths: Vec<String> = files
        .paths()
        .filter(|path| path.ends_with(".glsl"))
        .map(String::from)
        .collect();

    for path in paths {
        let bytes = files.get(&path).unwrap_or_default();
        let text = std::str::from_utf8(bytes).map_err(|e| UpgradeError::Malformed {
            file: path.clone(),
            detail: e.to_string(),
        })?;
        if let Some(rewritten) = rename_render_entry(text) {
            files.replace(&path, rewritten.into_bytes());
            report.record_changed(&path);
            report.note(format!(
                "{path}: entry `vec4 render(` → `vec4 render_2d(` (D19/Q11 explicit \
                 render entries — render is no longer a recognized shader entry)"
            ));
        }
    }
    Ok(())
}

/// Rename every `vec4 render(` entry definition to `vec4 render_2d(` in
/// `source`. Returns `None` when nothing matched (byte-identical), matching
/// the convention every step in this crate follows for untouched files.
fn rename_render_entry(source: &str) -> Option<String> {
    let mut out = String::with_capacity(source.len() + 4);
    let mut changed = false;
    let mut prev_char: Option<char> = None;
    let mut i = 0;
    while i < source.len() {
        let at_word_start = prev_char.is_none_or(|c| !is_ident_continue(c));
        if at_word_start {
            if let Some(match_len) = match_render_definition(&source[i..]) {
                let piece = &source[i..i + match_len];
                out.push_str(&piece.replacen("render", "render_2d", 1));
                i += match_len;
                changed = true;
                prev_char = Some('(' /* any non-ident char */);
                continue;
            }
        }
        let ch = source[i..].chars().next().expect("non-empty slice");
        out.push(ch);
        i += ch.len_utf8();
        prev_char = Some(ch);
    }
    changed.then_some(out)
}

/// If `s` starts with `vec4` · whitespace · `render` (each a whole word,
/// with `(` — optionally past more whitespace — right after), returns the
/// byte length of the matched span up to and including `render` (the
/// trailing whitespace and `(` are left for the caller to copy verbatim).
fn match_render_definition(s: &str) -> Option<usize> {
    let after_vec4 = match_word(s, "vec4")?;
    let ws1 = skip_ws(&s[after_vec4..]);
    if ws1 == 0 {
        return None;
    }
    let render_start = after_vec4 + ws1;
    let after_render = match_word(&s[render_start..], "render")?;
    let render_end = render_start + after_render;
    let ws2 = skip_ws(&s[render_end..]);
    if s[render_end + ws2..].starts_with('(') {
        Some(render_end)
    } else {
        None
    }
}

/// If `s` starts with the whole word `word` — not a prefix of a longer
/// identifier — returns `word.len()`.
fn match_word(s: &str, word: &str) -> Option<usize> {
    if !s.starts_with(word) {
        return None;
    }
    match s[word.len()..].chars().next() {
        Some(c) if is_ident_continue(c) => None,
        _ => Some(word.len()),
    }
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn skip_ws(s: &str) -> usize {
    s.chars()
        .take_while(|c| c.is_whitespace())
        .map(char::len_utf8)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_two_d_entry_is_renamed() {
        let source = "layout(binding = 0) uniform vec2 outputSize;\n\nvec4 render(vec2 pos) {\n    return vec4(1.0);\n}\n";
        let rewritten = rename_render_entry(source).expect("renamed");
        assert!(
            rewritten.contains("vec4 render_2d(vec2 pos) {"),
            "{rewritten}"
        );
        assert!(!rewritten.contains("render("), "{rewritten}");
    }

    #[test]
    fn a_comment_mentioning_render_survives_untouched() {
        let source = "// Naga GLSL-in resolves calls in source order; define helpers before render().\n\
                       // Virtual resolution: pattern matches a 32x32 render regardless of outputSize.\n\
                       vec4 render(vec2 pos) {\n    return vec4(0.0);\n}\n";
        let rewritten = rename_render_entry(source).expect("renamed");
        assert!(
            rewritten.contains("define helpers before render()."),
            "{rewritten}"
        );
        assert!(rewritten.contains("32x32 render regardless"), "{rewritten}");
        assert!(
            rewritten.contains("vec4 render_2d(vec2 pos) {"),
            "{rewritten}"
        );
    }

    #[test]
    fn a_shader_with_no_render_entry_is_untouched() {
        // Compute-shader assets: `void tick()`, no `render` at all.
        let source = "void tick() {\n    events[0].id = 1u;\n}\n";
        assert_eq!(rename_render_entry(source), None);
    }

    #[test]
    fn an_identifier_only_sharing_the_word_render_is_left_alone() {
        // `prerender` and `renderTarget` must not partially match.
        let source = "vec4 prerender(vec2 pos) { return vec4(0.0); }\nfloat renderTarget;\n";
        assert_eq!(rename_render_entry(source), None);
    }

    #[test]
    fn only_the_manifest_format_is_bumped() {
        let mut files: ProjectFiles = [(
            "project.json",
            b"{\n  \"format\": 6,\n  \"name\": \"x\"\n}".to_vec(),
        )]
        .into_iter()
        .collect();
        let mut report = UpgradeReport::new(FROM);
        apply(&mut files, &mut report).expect("upgrades");
        assert_eq!(
            files.get("project.json"),
            Some(b"{\n  \"format\": 7,\n  \"name\": \"x\"\n}\n".as_slice())
        );
    }

    #[test]
    fn a_glsl_asset_is_rewritten_alongside_the_manifest() {
        let mut files: ProjectFiles = [
            ("project.json", b"{\"format\": 6}".to_vec()),
            (
                "shader.glsl",
                b"vec4 render(vec2 pos) {\n    return vec4(1.0);\n}\n".to_vec(),
            ),
        ]
        .into_iter()
        .collect();
        let mut report = UpgradeReport::new(FROM);
        apply(&mut files, &mut report).expect("upgrades");
        let rewritten = std::str::from_utf8(files.get("shader.glsl").unwrap()).unwrap();
        assert!(
            rewritten.contains("vec4 render_2d(vec2 pos)"),
            "{rewritten}"
        );
        assert_eq!(
            report.changed_files,
            vec![String::from("project.json"), String::from("shader.glsl")]
        );
    }
}
