//! Source lint: every rsx `select` that binds `value:` must mirror the
//! selection onto its options with `selected:`.
//!
//! Mechanism (Dioxus 0.7 web): mounting a template applies an element's
//! dynamic attributes BEFORE its dynamic children exist. The interpreter
//! writes a select's `value` as the DOM property (`node.value = …`), which
//! matches nothing while the select has no `<option>` children — the write
//! lands as "", and once the options mount the browser falls back to
//! displaying the FIRST option regardless of the bound value. `selected` on
//! an `<option>` is also written as a DOM property, which the select honors
//! when the option is inserted — so mirroring the bound value onto each
//! option (`selected: option_value == bound_value`) renders the correct
//! initial selection, and the select-level `value:` keeps later re-renders
//! in sync. Visual pin: the `dropdown_field_wired` story captures a
//! non-first selection.
//!
//! This test walks the crate's rsx sources and fails on any `select` block
//! that binds a top-level `value:` without a `selected:` mirror inside it.
//! If a site genuinely cannot carry the mirror inline (options rendered by
//! a helper component that mirrors `selected:` itself), suppress with
//! `// select-mirror-lint: allow` on the line directly above `select {`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// One rsx `select { … }` block lifted out of a source file.
struct SelectBlock {
    /// 1-based line of the `select {` token.
    line: usize,
    /// Text of the block at brace depth 1 only — the select's own
    /// attributes, with nested children/closure bodies elided. `value:`
    /// here is the select's own binding, not an option's.
    own_level: String,
    /// Full text of the block, children included.
    full: String,
    /// A `// select-mirror-lint: allow` marker sits on the preceding line.
    allowed: bool,
}

#[test]
fn every_value_bound_select_mirrors_selected_onto_its_options() {
    let src_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sources = Vec::new();
    collect_rust_sources(&src_root, &mut sources);
    assert!(
        sources.len() > 50,
        "source walk looks broken: only {} .rs files under {}",
        sources.len(),
        src_root.display()
    );

    let mut value_bound = 0usize;
    let mut violations = String::new();
    for path in &sources {
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        for block in select_blocks(&text) {
            if !binds_word(&block.own_level, "value") {
                continue;
            }
            value_bound += 1;
            if block.allowed || binds_word(&block.full, "selected") {
                continue;
            }
            let rel = path.strip_prefix(&src_root).unwrap_or(path);
            writeln!(violations, "  src/{}:{}", rel.display(), block.line).unwrap();
        }
    }

    // Parser self-check: the crate is known to carry value-bound selects
    // (slot fields, settings, agent chat, package card, …). Finding fewer
    // means the scan broke, not that the problem went away.
    assert!(
        value_bound >= 6,
        "select scan looks broken: found only {value_bound} value-bound select blocks"
    );

    assert!(
        violations.is_empty(),
        "rsx select binds `value:` without mirroring `selected:` onto its options.\n\
         A select's `value` is applied before its options mount, so on first\n\
         render the browser shows the FIRST option instead of the bound one.\n\
         Add `selected: <option value> == <bound value>` to each option\n\
         (see DropdownSlotField in app/node/slot_fields.rs), or suppress with\n\
         `// select-mirror-lint: allow` if the options mirror it elsewhere.\n\
         Offending select blocks:\n{violations}"
    );
}

fn collect_rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|err| panic!("{}: {err}", dir.display()));
    for entry in entries {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_rust_sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    out.sort();
}

/// Whether `text` contains `word:` as a whole word (`value:` matches,
/// `initial_value:` does not).
fn binds_word(text: &str, word: &str) -> bool {
    let needle = format!("{word}:");
    let mut from = 0;
    while let Some(at) = text[from..].find(&needle) {
        let at = from + at;
        let boundary = at == 0
            || !text[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if boundary {
            return true;
        }
        from = at + needle.len();
    }
    false
}

/// Scan Rust source for rsx `select { … }` blocks, tracking string
/// literals, comments, and char literals so braces inside them do not skew
/// the depth count (rsx format strings like `"{value}"` are full of them).
fn select_blocks(text: &str) -> Vec<SelectBlock> {
    let bytes = text.as_bytes();
    let mut blocks = Vec::new();
    let mut i = 0;
    let mut line = 1usize;
    // Some((start_line, depth, own_level, full, allowed)) while inside one.
    let mut current: Option<SelectBlock> = None;
    let mut depth = 0usize;
    let mut last_line_was_allow = false;

    while i < bytes.len() {
        let ch = bytes[i];

        // Multi-byte UTF-8 (comment/label prose): pass through whole.
        if ch >= 0x80 {
            let c = text[i..].chars().next().expect("valid utf8");
            push_char(&mut current, depth, c);
            i += c.len_utf8();
            continue;
        }
        let rest = &text[i..];

        if ch == b'\n' {
            let ending = &text[..i];
            let line_text = ending[ending.rfind('\n').map_or(0, |p| p + 1)..].trim();
            last_line_was_allow = line_text == "// select-mirror-lint: allow";
            line += 1;
            push_char(&mut current, depth, '\n');
            i += 1;
            continue;
        }

        // Comments.
        if rest.starts_with("//") {
            let end = rest.find('\n').map_or(text.len(), |p| i + p);
            i = end;
            continue;
        }
        if rest.starts_with("/*") {
            let end = rest.find("*/").map_or(text.len(), |p| i + p + 2);
            line += text[i..end].matches('\n').count();
            i = end;
            continue;
        }

        // String literals (plain and raw). Contents are elided from the
        // captured block text; a "{value}" format brace must not count.
        if ch == b'"' {
            i = skip_string(text, i, &mut line);
            continue;
        }
        if rest.starts_with("r\"") || rest.starts_with("r#") {
            i = skip_raw_string(text, i, &mut line);
            continue;
        }

        // Char literals ('}' must not close a brace); lifetimes pass through.
        if ch == b'\'' {
            if let Some(end) = char_literal_end(text, i) {
                i = end;
                continue;
            }
        }

        if current.is_none() && rest.starts_with("select") {
            let before_ok = i == 0
                || !text[..i]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.');
            let after = text[i + "select".len()..].trim_start();
            if before_ok && after.starts_with('{') {
                current = Some(SelectBlock {
                    line,
                    own_level: String::new(),
                    full: String::new(),
                    allowed: last_line_was_allow,
                });
                depth = 0;
                i += "select".len();
                continue;
            }
        }

        match ch {
            b'{' => {
                depth += 1;
                push_char(&mut current, depth.saturating_sub(1), '{');
            }
            b'}' => {
                depth = depth.saturating_sub(1);
                push_char(&mut current, depth, '}');
                if depth == 0
                    && let Some(block) = current.take()
                {
                    blocks.push(block);
                }
            }
            _ => push_char(&mut current, depth, ch as char),
        }
        i += 1;
    }
    blocks
}

/// Append `ch` to the current block's captures: always to `full`, and to
/// `own_level` only at attribute depth (1).
fn push_char(current: &mut Option<SelectBlock>, depth: usize, ch: char) {
    if let Some(block) = current {
        block.full.push(ch);
        if depth == 1 {
            block.own_level.push(ch);
        }
    }
}

/// Byte index just past a plain string literal starting at `start` (`"`).
fn skip_string(text: &str, start: usize, line: &mut usize) -> usize {
    let bytes = text.as_bytes();
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            b'\n' => {
                *line += 1;
                i += 1;
            }
            _ => i += 1,
        }
    }
    text.len()
}

/// Byte index just past a raw string literal starting at `start` (`r` of
/// `r"…"` / `r#"…"#`).
fn skip_raw_string(text: &str, start: usize, line: &mut usize) -> usize {
    let hashes = text[start + 1..].chars().take_while(|&c| c == '#').count();
    let open = start + 1 + hashes;
    if text.as_bytes().get(open) != Some(&b'"') {
        // `r#ident` (raw identifier like r#type), not a raw string.
        return start + 1;
    }
    let close = format!("\"{}", "#".repeat(hashes));
    match text[open + 1..].find(&close) {
        Some(p) => {
            let end = open + 1 + p + close.len();
            *line += text[start..end].matches('\n').count();
            end
        }
        None => text.len(),
    }
}

/// End of a char literal at `start` (`'`), or `None` for a lifetime.
fn char_literal_end(text: &str, start: usize) -> Option<usize> {
    let rest = &text[start + 1..];
    let mut chars = rest.chars();
    match chars.next()? {
        '\\' => {
            // Escaped char: find the closing quote after the escape.
            let after = &rest[1..];
            after.find('\'').map(|p| start + 2 + p + 1)
        }
        c => {
            // `'x'` is a char literal; `'static` (no closing quote right
            // after one char) is a lifetime.
            let next_at = 1 + c.len_utf8();
            (rest.as_bytes().get(next_at) == Some(&b'\'')).then_some(start + 1 + next_at + 1)
        }
    }
}

#[test]
fn select_scanner_understands_the_idiom() {
    let good = r#"
        rsx! {
            select {
                class: "x",
                value: "{value}",
                oninput: move |event| { let v = event.value(); },
                for option in options {
                    option { value: "{option.v}", selected: option.v == value, "{option.label}" }
                }
            }
        }
    "#;
    let blocks = select_blocks(good);
    assert_eq!(blocks.len(), 1);
    assert!(binds_word(&blocks[0].own_level, "value"));
    assert!(binds_word(&blocks[0].full, "selected"));

    let bad = r#"
        rsx! {
            select {
                value: "{value}",
                onchange: move |event| chosen.set(event.value()),
                for name in names {
                    option { value: "{name}", "{name}" }
                }
            }
        }
    "#;
    let blocks = select_blocks(bad);
    assert_eq!(blocks.len(), 1);
    assert!(binds_word(&blocks[0].own_level, "value"));
    assert!(
        !binds_word(&blocks[0].full, "selected"),
        "the handler's event.value() must not read as a selected mirror"
    );

    // The option's own `value:` must not satisfy the select-level check,
    // and `initial_value:` is not `value:`.
    let value_free = r#"
        select {
            initial_value: "x",
            for name in names {
                option { value: "{name}", "{name}" }
            }
        }
    "#;
    let blocks = select_blocks(value_free);
    assert_eq!(blocks.len(), 1);
    assert!(!binds_word(&blocks[0].own_level, "value"));

    // Suppression marker.
    let allowed = r#"
        // select-mirror-lint: allow
        select {
            value: "{value}",
            OptionRows { value }
        }
    "#;
    let blocks = select_blocks(allowed);
    assert_eq!(blocks.len(), 1);
    assert!(blocks[0].allowed);
}
