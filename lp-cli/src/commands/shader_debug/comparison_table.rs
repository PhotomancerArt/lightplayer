//! Instruction-count summary: fixed-width text table (no external table crate).

use unicode_width::UnicodeWidthStr;

use super::types::DebugReport;

const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const RESET: &str = "\x1b[0m";

/// Strip CSI `ESC [ ... m` sequences (SGR) for width measurement only.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\x1b' && it.peek() == Some(&'[') {
            it.next();
            for ch in it.by_ref() {
                if ch == 'm' {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn visible_width(s: &str) -> usize {
    strip_ansi(s).width()
}

#[derive(Clone, Copy)]
enum ColAlign {
    Left,
    Right,
}

/// Pad `s` (may contain ANSI) to `target` **display** width.
fn pad_cell(s: &str, target: usize, align: ColAlign) -> String {
    let w = visible_width(s);
    let pad = target.saturating_sub(w);
    let spaces = " ".repeat(pad);
    match align {
        ColAlign::Left => format!("{s}{spaces}"),
        ColAlign::Right => format!("{spaces}{s}"),
    }
}

/// Render a pipe table: `rows[0]` is the header; a separator line is inserted after it.
fn render_table(rows: &[Vec<String>], align: &[ColAlign]) -> String {
    assert!(!rows.is_empty());
    let cols = align.len();
    assert!(rows.iter().all(|r| r.len() == cols));

    let mut widths = vec![0usize; cols];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(visible_width(cell));
        }
    }

    let mut out = String::new();
    for (ri, row) in rows.iter().enumerate() {
        let cells: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, c)| pad_cell(c, widths[i], align[i]))
            .collect();
        out.push_str("| ");
        out.push_str(&cells.join(" | "));
        out.push_str(" |\n");

        if ri == 0 {
            let seps: Vec<String> = widths.iter().map(|w| "-".repeat((*w).max(3))).collect();
            out.push_str("| ");
            out.push_str(&seps.join(" | "));
            out.push_str(" |\n");
        }
    }
    out
}

fn color_for_ratio(ratio: f64) -> &'static str {
    if ratio <= 1.0005 {
        GREEN
    } else if ratio <= 1.2 {
        YELLOW
    } else {
        RED
    }
}

fn ratio_text(ratio: f64) -> String {
    if ratio <= 1.0005 {
        "1.00×".to_string()
    } else {
        format!("{ratio:.2}×")
    }
}

fn format_count_with_ratio(
    count: usize,
    min_count: usize,
    multi_backend: bool,
    use_color: bool,
) -> String {
    if !multi_backend || min_count == 0 {
        return count.to_string();
    }
    let ratio = count as f64 / min_count as f64;
    let rt = ratio_text(ratio);
    let ratio_part = if use_color {
        format!("{}{}{}", color_for_ratio(ratio), rt, RESET)
    } else {
        rt
    };
    format!("{count} ({ratio_part})")
}

fn legend_line(use_color: bool) -> String {
    if use_color {
        format!(
            "(fastest is baseline - {}{}{} {}{}{} {}{}{})",
            GREEN, "green = best", RESET, YELLOW, "yellow <= 1.2x", RESET, RED, "red > 1.2x", RESET
        )
    } else {
        "(fastest is baseline - green = best yellow <= 1.2x red > 1.2x)".to_string()
    }
}

/// Render the summary block (title + table + optional legend), or `None` if there is nothing to show.
pub fn render_summary_table(
    report: &DebugReport,
    use_color: bool,
    show_weights: bool,
) -> Option<String> {
    if report.backends.is_empty() {
        return None;
    }

    let func_names = report.function_names();
    if func_names.is_empty() {
        return None;
    }

    let n = report.backends.len();

    let mut align = vec![ColAlign::Left, ColAlign::Right];
    if show_weights {
        align.extend(std::iter::repeat(ColAlign::Right).take(3));
    }
    align.extend(std::iter::repeat(ColAlign::Right).take(n));

    let mut header: Vec<String> = vec!["Function".to_string(), "LPIR".to_string()];
    if show_weights {
        header.push("body_len".to_string());
        header.push("mz".to_string());
        header.push("hb".to_string());
    }
    for b in &report.backends {
        header.push(b.backend.clone());
    }

    let mut rows: Vec<Vec<String>> = vec![header];

    let mut total_lpir = 0usize;
    let mut total_body_len = 0usize;
    let mut total_mz = 0usize;
    let mut total_hb = 0usize;
    let mut total_disasm: Vec<usize> = vec![0; n];

    for func_name in &func_names {
        let first_fd = report
            .backends
            .first()
            .and_then(|b| b.get_function(func_name));

        let lpir_count = first_fd.map(|f| f.lpir_count).unwrap_or(0);
        total_lpir += lpir_count;

        let (w_bl, w_mz, w_hb) = if show_weights {
            first_fd
                .map(|f| (f.weight_body_len, f.weight_mz, f.weight_hb))
                .unwrap_or((0, 0, 0))
        } else {
            (0usize, 0usize, 0usize)
        };
        if show_weights {
            total_body_len += w_bl;
            total_mz += w_mz;
            total_hb += w_hb;
        }

        let mut disasm = Vec::with_capacity(n);
        for backend in &report.backends {
            let d = backend
                .get_function(func_name)
                .map(|f| f.disasm_count)
                .unwrap_or(0);
            disasm.push(d);
        }
        for (i, d) in disasm.iter().enumerate() {
            total_disasm[i] += d;
        }

        let min_d = disasm.iter().copied().min().unwrap_or(0);
        let multi = n > 1;

        let mut row: Vec<String> = vec![(*func_name).to_string(), lpir_count.to_string()];
        if show_weights {
            row.push(w_bl.to_string());
            row.push(w_mz.to_string());
            row.push(w_hb.to_string());
        }
        for d in &disasm {
            row.push(format_count_with_ratio(*d, min_d, multi, use_color));
        }
        rows.push(row);
    }

    let min_t = total_disasm.iter().copied().min().unwrap_or(0);
    let multi = n > 1;

    let mut total_row: Vec<String> = vec!["TOTAL".to_string(), total_lpir.to_string()];
    if show_weights {
        total_row.push(total_body_len.to_string());
        total_row.push(total_mz.to_string());
        total_row.push(total_hb.to_string());
    }
    for t in &total_disasm {
        total_row.push(format_count_with_ratio(*t, min_t, multi, use_color));
    }
    rows.push(total_row);

    let title = format!(
        "=== Summary: {} {} ===\n\n",
        n,
        if n == 1 { "target" } else { "targets" }
    );

    let mut out = title;
    out.push_str(&render_table(&rows, &align));
    if n > 1 {
        out.push('\n');
        out.push_str(&legend_line(use_color));
        out.push('\n');
    }
    out.push('\n');
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::shader_debug::types::{BackendDebugData, FunctionDebugData};

    #[test]
    fn pad_preserves_ansi_visible_width() {
        let s = format!("{}hi{}", GREEN, RESET);
        assert_eq!(visible_width(&s), 2);
        let p = pad_cell(&s, 5, ColAlign::Right);
        assert_eq!(visible_width(&p), 5);
        assert!(p.ends_with(&s) || p.contains("hi"));
    }

    #[test]
    fn summary_contains_counts_and_ratios_no_color() {
        let mut rv32c = BackendDebugData::new("rv32c");
        let mut f0 = FunctionDebugData::new("callee_identity".to_string());
        f0.lpir_count = 3;
        f0.disasm_count = 2;
        rv32c.functions.push(f0);

        let mut rv32n = BackendDebugData::new("rv32n");
        let mut f1 = FunctionDebugData::new("callee_identity".to_string());
        f1.lpir_count = 3;
        f1.disasm_count = 9;
        rv32n.functions.push(f1);

        let mut r = DebugReport::new();
        r.backends.push(rv32c);
        r.backends.push(rv32n);

        let s = render_summary_table(&r, false, false).expect("table");
        assert!(!s.contains('\x1b'), "no ansi when use_color=false:\n{s}");
        assert!(s.contains("callee_identity"));
        assert!(s.contains("2 (1.00×)"));
        assert!(s.contains("9 (4.50×)"));
        assert!(s.contains("TOTAL"));
    }

    #[test]
    fn summary_includes_weight_columns_when_requested() {
        let mut rv32c = BackendDebugData::new("rv32c");
        let mut f0 = FunctionDebugData::new("foo".to_string());
        f0.lpir_count = 10;
        f0.weight_body_len = 10;
        f0.weight_mz = 6;
        f0.weight_hb = 8;
        f0.disasm_count = 3;
        rv32c.functions.push(f0);

        let mut rv32n = BackendDebugData::new("rv32n");
        let mut f1 = FunctionDebugData::new("foo".to_string());
        f1.lpir_count = 10;
        f1.weight_body_len = 10;
        f1.weight_mz = 6;
        f1.weight_hb = 8;
        f1.disasm_count = 12;
        rv32n.functions.push(f1);

        let mut r = DebugReport::new();
        r.backends.push(rv32c);
        r.backends.push(rv32n);

        let s = render_summary_table(&r, false, true).expect("table");
        assert!(s.contains("body_len"));
        assert!(s.contains("mz"));
        assert!(s.contains("hb"));
        assert!(s.contains("foo"));
        assert!(s.contains("TOTAL"));
        assert!(s.lines().any(|line| line.contains("foo") && line.contains("10") && line.contains("6") && line.contains("8")));
    }
}
