//! Renders a [`FragAnalysis`] as the `lp-cli profile` `Heap Fragmentation`
//! report section, and a [`CounterfactualReport`] as the
//! `Heap Counterfactuals` one. `frag.json` and `frag-cf.json` carry the full
//! per-marker detail; this text is the part meant to be read.

use ::alloc::format;
use ::alloc::string::{String, ToString};
use ::alloc::vec::Vec;
use std::fmt::Write as _;

use super::frag_counterfactual::CounterfactualReport;
use super::frag_replay::{FRAME_ALLOC_OF_INTEREST, FragAnalysis, HISTOGRAM_LABELS};

/// How many pinning residents and would-OOM rows the text section lists.
const REPORT_ROWS: usize = 10;

/// Render the whole section body (without the `=== … ===` banner).
pub fn render_fragmentation_section(analysis: &FragAnalysis) -> String {
    let mut out = String::new();
    render_layout(&mut out, analysis);
    render_markers(&mut out, analysis);
    render_histogram(&mut out, analysis);
    render_tightest_marker(&mut out, analysis);
    render_pinning(&mut out, analysis);
    render_would_oom(&mut out, analysis);
    render_sized_allocs(&mut out, analysis);
    render_cross_check(&mut out, analysis);
    out
}

/// Render the counterfactual table (without the `=== … ===` banner): one row
/// per lever, baseline first, one block of columns per marker that matters.
///
/// The table is wide, so it is printed as one stanza per row rather than one
/// line — a line long enough to hold five markers × five figures wraps in
/// every terminal and stops being readable at all.
pub fn render_counterfactual_section(report: &CounterfactualReport) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Layout {} — {} region(s); every row is the same trace replayed with one lever \
         already pulled",
        report.layout,
        report.regions.len()
    );
    render_discount_line(&mut out, report);
    if report.columns.is_empty() {
        let _ = writeln!(
            out,
            "  (no project-load, shader-compile, frame or project-read window closed in this \
             trace — nothing to tabulate)"
        );
        return out;
    }
    let last = report
        .columns
        .last()
        .expect("just checked the column list is not empty");
    out.push('\n');

    let _ = writeln!(
        out,
        "  {:<34} {:>11} {:>11} {:>11} {:>7} {:>11}",
        "counterfactual @ marker", "largest", "r0 largest", "r1 largest", "holes", "free"
    );
    for row in &report.rows {
        let _ = writeln!(
            out,
            "  {} [{}]{}",
            row.label,
            row.engine,
            if row.would_oom > 0 {
                format!("  ⚠ {} would-OOM", fmt_num(row.would_oom))
            } else {
                String::new()
            }
        );
        for cell in &row.cells {
            let _ = writeln!(
                out,
                "  {:<34} {:>11} {:>11} {:>11} {:>7} {:>11}",
                format!("    {}", truncate(&cell.column, 30)),
                fmt_num(u64::from(cell.largest)),
                fmt_num(u64::from(cell.region_largest.first().copied().unwrap_or(0))),
                fmt_num(u64::from(cell.region_largest.get(1).copied().unwrap_or(0))),
                cell.holes,
                fmt_num(u64::from(cell.free)),
            );
        }
        if row.label != "baseline" {
            let _ = writeln!(
                out,
                "    Δ largest at {}: {}{}",
                last.label,
                if row.delta_largest_last >= 0 { "+" } else { "" },
                fmt_signed(row.delta_largest_last),
            );
        }
        for note in &row.notes {
            let _ = writeln!(out, "    note: {note}");
        }
        for approximation in &row.approximations {
            let _ = writeln!(out, "    approximation: {approximation}");
        }
        out.push('\n');
    }
    out
}

fn render_discount_line(out: &mut String, report: &CounterfactualReport) {
    if report.discounts.is_empty() {
        let _ = writeln!(
            out,
            "  discounts: none — every allocation in the trace is replayed"
        );
        return;
    }
    let patterns = report
        .discounts
        .iter()
        .map(|d| d.pattern.clone())
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        out,
        "  ⚠ DISCOUNTED TABLE: dropped call site pattern(s): {patterns}"
    );
}

fn render_layout(out: &mut String, analysis: &FragAnalysis) {
    let total: u64 = analysis.regions.iter().map(|r| u64::from(r.size)).sum();
    let _ = writeln!(
        out,
        "Layout {} — {} region(s), {} B total",
        analysis.layout,
        analysis.regions.len(),
        fmt_num(total),
    );
    for region in &analysis.regions {
        let _ = writeln!(
            out,
            "  region {}: {} B at 0x{:08x}",
            region.index,
            fmt_num(u64::from(region.size)),
            region.base
        );
    }

    let alignments = analysis
        .alignments
        .iter()
        .map(|(align, count)| format!("{align} B x{}", fmt_num(*count)))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        out,
        "  request alignment recorded per row: {alignments}{}",
        if analysis.alignments.len() == 1 && analysis.alignments.contains_key(&4) {
            "  (only 4 B — a trace recorded before the `al` field existed reads this way too)"
        } else {
            ""
        }
    );

    render_discounts(out, analysis);

    if analysis.pointer_collisions > 0 {
        let _ = writeln!(
            out,
            "  ⚠ {} allocation(s) of a pointer that was already live — the live set silently \
             dropped the older block, so every figure below under-counts",
            analysis.pointer_collisions
        );
    }
    if analysis.unmatched_frees > 0 {
        let _ = writeln!(
            out,
            "  ⚠ {} free(s) of pointers this replay never saw allocated — the trace \
             does not start from an empty heap, so every figure below is optimistic",
            analysis.unmatched_frees
        );
    }
    out.push('\n');
}

/// Name every discount at the top of the section, with what it removed, so a
/// discounted table can never be read as a raw one — and say so explicitly
/// when there are none.
fn render_discounts(out: &mut String, analysis: &FragAnalysis) {
    if analysis.discounts.is_empty() {
        let _ = writeln!(
            out,
            "  discounts: none — every allocation in the trace is replayed"
        );
        return;
    }
    let _ = writeln!(
        out,
        "  ⚠ DISCOUNTED TABLE: {} call site pattern(s) dropped from the replay",
        analysis.discounts.len()
    );
    for row in &analysis.discounts {
        let _ = writeln!(
            out,
            "      -{:<48} {:>7} blocks, {:>11} B requested, {:>9} B peak live",
            row.pattern,
            fmt_num(row.blocks),
            fmt_num(row.bytes_requested),
            fmt_num(row.peak_live_bytes),
        );
        if row.blocks == 0 {
            let _ = writeln!(
                out,
                "       (matched nothing — check the pattern against the pinning table's site names)"
            );
        }
    }
}

fn render_markers(out: &mut String, analysis: &FragAnalysis) {
    let _ = writeln!(out, "Free space at each marker");
    let mut header = format!(
        "  {:<24} {:>4} {:>11} {:>7} {:>11} {:>11}",
        "marker", "kind", "largest", "holes", "free", "live"
    );
    for region in &analysis.regions {
        let _ = write!(header, " {:>11}", format!("r{} largest", region.index));
    }
    let _ = writeln!(out, "{header}");

    for marker in &analysis.markers {
        let mut row = format!(
            "  {:<24} {:>4} {:>11} {:>7} {:>11} {:>11}",
            truncate(&marker.name, 24),
            marker.kind,
            fmt_num(u64::from(marker.largest)),
            marker.holes,
            fmt_num(u64::from(marker.free)),
            fmt_num(marker.live_bytes),
        );
        for region in &marker.regions {
            let _ = write!(row, " {:>11}", fmt_num(u64::from(region.largest)));
        }
        let _ = writeln!(out, "{row}");
    }
    out.push('\n');
}

fn render_histogram(out: &mut String, analysis: &FragAnalysis) {
    let _ = writeln!(
        out,
        "Hole histogram (count per power-of-two bucket, lower bound shown)"
    );
    let mut header = format!("  {:<24}", "marker");
    for label in HISTOGRAM_LABELS {
        let _ = write!(header, " {label:>5}");
    }
    let _ = writeln!(out, "{header}");
    for marker in &analysis.markers {
        let mut row = format!("  {:<24}", truncate(&marker.name, 24));
        for count in marker.histogram {
            if count == 0 {
                let _ = write!(row, " {:>5}", ".");
            } else {
                let _ = write!(row, " {count:>5}");
            }
        }
        let _ = writeln!(out, "{row}");
    }
    out.push('\n');
}

/// The marker where the largest free block is smallest: the moment the layout
/// is closest to refusing a big contiguous ask, and therefore the one worth
/// printing the bounding blocks for.
fn render_tightest_marker(out: &mut String, analysis: &FragAnalysis) {
    let Some(tightest) = analysis.markers.iter().min_by_key(|m| m.largest) else {
        return;
    };
    let _ = writeln!(
        out,
        "Top holes at the tightest marker ({} {}, ic {}) — largest free {} B",
        tightest.name,
        tightest.kind,
        tightest.ic,
        fmt_num(u64::from(tightest.largest))
    );
    let _ = writeln!(
        out,
        "  {:>2} {:>9} {:>2}  {:<34} {:<34}",
        "#", "hole", "rg", "block below (size, site, born)", "block above (size, site, born)"
    );
    for (rank, hole) in tightest.top_holes.iter().enumerate() {
        let _ = writeln!(
            out,
            "  {:>2} {:>9} {:>2}  {:<34} {:<34}",
            rank + 1,
            fmt_num(u64::from(hole.size)),
            hole.region,
            hole.below
                .as_ref()
                .map_or_else(|| "(region bottom)".into(), describe_bound),
            hole.above
                .as_ref()
                .map_or_else(|| "(region top)".into(), describe_bound),
        );
    }
    out.push('\n');
}

fn describe_bound(block: &super::frag_replay::BoundingBlock) -> String {
    format!(
        "{} B {} [{} +{}ic]",
        fmt_num(u64::from(block.size)),
        truncate(&block.site, 44),
        block.born_window,
        fmt_num(block.age_ic)
    )
}

fn render_pinning(out: &mut String, analysis: &FragAnalysis) {
    let _ = writeln!(
        out,
        "Pinning residents by call site (blocks that bounded a top hole, all markers)"
    );
    let _ = writeln!(
        out,
        "  {:>11} {:>7} {:>7} {:>13}  {}",
        "bytes live", "blocks", "holes", "hole bytes", "site"
    );
    for row in analysis.pinning.iter().take(REPORT_ROWS) {
        let _ = writeln!(
            out,
            "  {:>11} {:>7} {:>7} {:>13}  {}",
            fmt_num(row.bytes_live),
            row.blocks,
            row.holes_bordered,
            fmt_num(row.hole_bytes_bordered),
            row.site
        );
    }
    if analysis.pinning.is_empty() {
        let _ = writeln!(out, "  (no live blocks bounded a reported hole)");
    }
    out.push('\n');
}

fn render_would_oom(out: &mut String, analysis: &FragAnalysis) {
    if analysis.would_oom.is_empty() {
        let _ = writeln!(
            out,
            "would-OOM: none — every allocation the guest served fits this layout too\n"
        );
        return;
    }
    let _ = writeln!(
        out,
        "would-OOM: {} allocation(s) this layout could not serve although the guest's \
         larger heap did (skipped, so everything after is optimistic)",
        analysis.would_oom.len()
    );
    let _ = writeln!(
        out,
        "  {:>9} {:>13} {:<16} {:<16} {}",
        "size", "ic", "window", "after marker", "site"
    );
    for row in analysis.would_oom.iter().take(REPORT_ROWS) {
        let _ = writeln!(
            out,
            "  {:>9} {:>13} {:<16} {:<16} {}",
            fmt_num(u64::from(row.size)),
            fmt_num(row.ic),
            truncate(&row.window, 16),
            truncate(&row.after_marker, 16),
            row.site
        );
    }
    if analysis.would_oom.len() > REPORT_ROWS {
        let _ = writeln!(
            out,
            "  … {} more in frag.json",
            analysis.would_oom.len() - REPORT_ROWS
        );
    }
    out.push('\n');
}

fn render_sized_allocs(out: &mut String, analysis: &FragAnalysis) {
    let _ = writeln!(
        out,
        "Allocations of exactly {} B inside a `frame` window",
        fmt_num(u64::from(FRAME_ALLOC_OF_INTEREST))
    );
    if analysis.frame_alloc_of_interest.is_empty() {
        let _ = writeln!(out, "  (none in this trace)\n");
        return;
    }
    if !analysis.discounts.is_empty() {
        let _ = writeln!(
            out,
            "  (attribution over the raw trace — a discounted site still appears here, \
             it is simply absent from the tables above)"
        );
    }
    for site in &analysis.frame_alloc_of_interest {
        let _ = writeln!(
            out,
            "  x{} in {} (first at ic {}): {}",
            site.count,
            site.window,
            fmt_num(site.first_ic),
            site.site
        );
        let _ = writeln!(out, "      {}", site.callstack);
    }
    out.push('\n');
}

fn render_cross_check(out: &mut String, analysis: &FragAnalysis) {
    let Some(rows) = &analysis.cross_check else {
        let _ = writeln!(
            out,
            "Cross-check against the guest's own free-list walk: run with \
             --frag-layout guest (this replay is on the {} layout)",
            analysis.layout
        );
        return;
    };
    let _ = writeln!(
        out,
        "Cross-check: replay vs the guest's own walk (tolerance: holes ±2, largest ±64 B)"
    );
    let _ = writeln!(
        out,
        "  replay columns are quantized to what that walk can see; `exact` is the true hole set"
    );
    let _ = writeln!(
        out,
        "  {:<24} {:>4} {:>7} {:>7} {:>7} {:>11} {:>11} {:>8}  {:>7} {:>11}  {}",
        "marker",
        "kind",
        "holes",
        "guest",
        "Δ",
        "largest",
        "guest",
        "Δ",
        "exact",
        "exact",
        "verdict"
    );
    let mut worst_holes = 0i64;
    let mut worst_largest = 0i64;
    for row in rows {
        worst_holes = worst_holes.max(row.hole_drift().abs());
        worst_largest = worst_largest.max(row.largest_drift().abs());
        let _ = writeln!(
            out,
            "  {:<24} {:>4} {:>7} {:>7} {:>7} {:>11} {:>11} {:>8}  {:>7} {:>11}  {}",
            truncate(&row.marker, 24),
            row.kind,
            row.replay_holes,
            row.guest_holes,
            row.hole_drift(),
            fmt_num(u64::from(row.replay_largest)),
            fmt_num(u64::from(row.guest_largest)),
            row.largest_drift(),
            row.replay_holes_exact,
            fmt_num(u64::from(row.replay_largest_exact)),
            if row.within_tolerance() {
                "ok"
            } else {
                "DRIFT"
            }
        );
    }
    let _ = writeln!(
        out,
        "  worst drift: holes ±{worst_holes}, largest ±{worst_largest} B"
    );
}

fn truncate(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        return s.into();
    }
    let kept: String = s.chars().take(width.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// [`fmt_num`] for a delta, which may be negative.
fn fmt_signed(n: i64) -> String {
    format!(
        "{}{}",
        if n < 0 { "-" } else { "" },
        fmt_num(n.unsigned_abs())
    )
}

/// Thousands separators, so a 186,368 in a column of six-digit numbers is
/// readable at a glance.
fn fmt_num(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    let chars: Vec<char> = digits.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(*c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_separators() {
        assert_eq!(fmt_num(0), "0");
        assert_eq!(fmt_num(999), "999");
        assert_eq!(fmt_num(1_000), "1,000");
        assert_eq!(fmt_num(186_368), "186,368");
    }

    #[test]
    fn truncation_keeps_the_column_width() {
        assert_eq!(truncate("short", 24), "short");
        assert_eq!(truncate("abcdef", 4).chars().count(), 4);
    }
}
