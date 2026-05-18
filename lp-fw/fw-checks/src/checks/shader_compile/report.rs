use alloc::format;
use alloc::string::String;

use super::records::ShaderCompileRecord;

pub fn report_from_jsonl(records_jsonl: &str) -> Result<Option<String>, serde_json::Error> {
    let mut cases = Vec::new();
    let mut total = None;
    for line in records_jsonl.lines().filter(|line| !line.trim().is_empty()) {
        match serde_json::from_str::<ShaderCompileRecord>(line)? {
            ShaderCompileRecord::CaseSummary(summary) => cases.push(summary),
            ShaderCompileRecord::TotalSummary(summary) => total = Some(summary),
        }
    }
    if cases.is_empty() && total.is_none() {
        return Ok(None);
    }

    let mut out = String::from("# Firmware Check Report\n\n");
    if let Some(total) = total {
        out.push_str(&format!(
            "Total build: {}\nWorst slice: {}\nWorst peak: {}\nCases: {}\n\n",
            fmt_ms_1(total.build_us),
            fmt_ms_1(total.worst_slice_us),
            fmt_kib_1(total.worst_peak_used),
            total.cases,
        ));
    }
    for case in cases {
        out.push_str(&format!(
            "- `{}`: build={}, ticks={}, max_slice={} [{}], peak={}, resident={}, after_drop={}\n",
            case.case,
            fmt_ms_1(case.build_us),
            case.ticks,
            fmt_ms_1(case.max_slice_us),
            fmt_stage(&case.max_slice_stage),
            fmt_kib_1(case.peak_used),
            fmt_kib_1(case.resident_used),
            fmt_kib_1(case.after_drop_used),
        ));
    }
    Ok(Some(out))
}

fn fmt_ms_1(us: u64) -> String {
    let tenths_ms = (us + 50) / 100;
    format!("{}.{}ms", tenths_ms / 10, tenths_ms % 10)
}

fn fmt_kib_1(bytes: usize) -> String {
    let tenths_kib = (bytes.saturating_mul(10) + 512) / 1024;
    format!("{}.{}KiB", tenths_kib / 10, tenths_kib % 10)
}

fn fmt_stage(stage: &str) -> &str {
    if stage.is_empty() { "unknown" } else { stage }
}

#[cfg(test)]
mod tests {
    use super::report_from_jsonl;

    #[test]
    fn summarizes_shader_compile_records() {
        let report = report_from_jsonl(
            r#"{"kind":"case-summary","check":"shader-compile-stress","case":"examples-basic","build_us":256200,"ticks":79,"max_slice_us":55400,"max_slice_stage":"Frontend::LowerLpir","peak_used":81088,"resident_used":22800,"after_drop_used":6324}
{"kind":"total-summary","check":"shader-compile-stress","build_us":256200,"cases":1,"worst_slice_us":55400,"worst_peak_used":81088}
"#,
        )
        .unwrap()
        .unwrap();

        assert!(report.contains("Total build: 256.2ms"));
        assert!(report.contains("Worst slice: 55.4ms"));
        assert!(report.contains("Worst peak: 79.2KiB"));
        assert!(report.contains("`examples-basic`"));
        assert!(report.contains("[Frontend::LowerLpir]"));
    }
}
