//! Frontend peak-heap sweep over the whole filetest corpus.
//!
//! The example probes (`lpc-engine/tests/example_shader_compile_peak_memory.rs`,
//! `lpvm-native/tests/xt_compile_peak_memory.rs`) measure what the product
//! ships; this one measures what the language *allows*: every filetest that
//! the lps-glsl frontend is expected to compile on `rv32lpn.q32` goes through
//! the staged frontend (lex → index → body → build-hir → lower-lpir) under
//! the shared byte-tracking allocator (`lpvm-native/tests/support/peak_alloc.rs`),
//! and the ranked table says which language shapes cost the most memory per
//! source byte and which leave the most resident when the HIR build ends.
//! Frontend only — the backend's per-function passes are the example
//! probes' job.
//!
//! ONE `#[test]`: the allocator counters are process-wide.

#[path = "../../lpvm-native/tests/support/peak_alloc.rs"]
mod peak_alloc;

use std::path::{Path, PathBuf};

use lps_filetests::discovery::discover_test_files;
use lps_filetests::parse::parse_test_file;
use lps_filetests::parse::test_type::TestType;
use lps_filetests::targets::{Disposition, Target, directive_disposition};

use peak_alloc::{StepRecord, Summary, TrackingAlloc, live, trace_frontend};

#[global_allocator]
static ALLOC: TrackingAlloc = TrackingAlloc;

fn filetests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("filetests")
}

fn trace(
    glsl: &str,
    texture_specs: &lps_filetests::parse::test_type::TextureSpecs,
    records: &mut Vec<StepRecord>,
) -> Result<usize, String> {
    let options = lps_glsl::CompileOptions {
        texture_specs: texture_specs.clone(),
        texel_fetch_bounds: lpir::TexelFetchBoundsMode::default(),
    };
    let output = trace_frontend(glsl, options, records)?;
    Ok(output.ir.functions.len())
}

#[test]
fn filetest_corpus_frontend_peaks() {
    // Ceiling: measured 2026-09-02 over the corpus; see the planning notes
    // (2026-09-02-0817-hir-per-node-copies-corpus). Set at ~1.4× the largest
    // file's peak, rounded up to a KB. Raise deliberately, never casually.
    // Largest: struct/deep-nested.glsl at 382,991 B (build-hir, fn16) — a
    // 5.3 KB file of three-level nested structs; see the notes for why.
    const FRONTEND_CEILING_BYTES: usize = 524 * 1024;

    let dir = filetests_dir();
    let target = Target::from_name("rv32lpn.q32").expect("lps-glsl device target");
    let files = discover_test_files(&dir).expect("discover filetests");
    assert!(
        files.len() > 500,
        "corpus discovery found only {} files",
        files.len()
    );

    let mut rows: Vec<Summary> = Vec::new();
    let mut skipped_error_type = 0usize;
    let mut skipped_disposition = 0usize;
    let mut skipped_parse = 0usize;
    let mut failed: Vec<(String, String)> = Vec::new();
    let mut records: Vec<StepRecord> = Vec::with_capacity(1024);

    for path in &files {
        let label = path
            .strip_prefix(&dir)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();
        let file = match parse_test_file(path) {
            Ok(file) => file,
            Err(_) => {
                skipped_parse += 1;
                continue;
            }
        };
        if file
            .test_types
            .iter()
            .any(|t| matches!(t, TestType::Error | TestType::ParseError))
        {
            skipped_error_type += 1;
            continue;
        }
        let annotations: Vec<_> = file
            .file_annotations
            .iter()
            .chain(
                file.run_directives
                    .iter()
                    .flat_map(|d| d.annotations.iter()),
            )
            .cloned()
            .collect();
        if directive_disposition(&annotations, target) != Disposition::ExpectSuccess {
            skipped_disposition += 1;
            continue;
        }

        // Warm-up (lazy process allocations), then the measured pass.
        records.clear();
        let warm = trace(&file.glsl_source, &file.texture_specs, &mut records);
        records.clear();
        let baseline = live();
        let result = trace(&file.glsl_source, &file.texture_specs, &mut records);
        match (warm, result) {
            (Ok(a), Ok(b)) => assert_eq!(a, b, "{label}: compile is deterministic"),
            (_, Err(err)) | (Err(err), _) => {
                failed.push((label, err.lines().next().unwrap_or("").to_string()));
                continue;
            }
        }
        assert!(
            records.len() < 1024,
            "record buffer reallocated mid-measurement"
        );
        rows.push(peak_alloc::summarize(
            label,
            "filetest",
            file.glsl_source.len(),
            baseline,
            &records,
        ));
    }

    peak_alloc::print_table(
        "filetest frontend peaks (host bytes above baseline)",
        &rows,
        15,
    );
    println!(
        "\nmeasured {} files; skipped {} (test error/parse-error), {} (disposition on {}), {} (unparseable); {} failed to compile",
        rows.len(),
        skipped_error_type,
        skipped_disposition,
        target.name(),
        skipped_parse,
        failed.len()
    );
    for (label, err) in failed.iter().take(10) {
        println!("  failed: {label}: {err}");
    }
    assert!(rows.len() > 400, "only {} filetests measured", rows.len());
    for r in &rows {
        assert!(
            r.peak <= FRONTEND_CEILING_BYTES,
            "{}: frontend transient peak {} B exceeds the {FRONTEND_CEILING_BYTES} B ceiling ({})",
            r.label,
            r.peak,
            r.peak_stage
        );
    }
}
