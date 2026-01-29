mod cli;
mod codegen;
mod compiler;
mod report;
mod stats;

use anyhow::Result;
use compiler::compile_and_transform;
use report::generate_reports;
use std::fs;
use std::path::Path;

fn main() -> Result<()> {
    let args = cli::parse_args();

    if args.verbose {
        eprintln!("Tests directory: {}", args.tests_dir.display());
        eprintln!("Output directory: {}", args.output_dir.display());
        eprintln!("Format: {:?}", args.format);
    }

    // Create output directory if it doesn't exist
    fs::create_dir_all(&args.output_dir)?;

    // Get all GLSL files from tests directory
    let glsl_files = find_glsl_files(&args.tests_dir)?;

    if glsl_files.is_empty() {
        anyhow::bail!("No GLSL files found in {}", args.tests_dir.display());
    }

    if args.verbose {
        eprintln!("Found {} GLSL files", glsl_files.len());
    }

    // Create timestamped report directory with report name
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H.%M.%S");
    let report_dir_name = format!("{}-{}", timestamp, args.report_name);
    let report_dir = args.output_dir.join(report_dir_name);
    fs::create_dir_all(&report_dir)?;

    if args.verbose {
        eprintln!("Report directory: {}", report_dir.display());
    }

    // Process each GLSL file
    let mut test_summaries = Vec::new();
    for glsl_file in &glsl_files {
        if args.verbose {
            eprintln!("Processing: {}", glsl_file.display());
        }

        let test_name = glsl_file.file_name().unwrap().to_string_lossy().to_string();
        let test_dir = report_dir.join(&test_name);
        fs::create_dir_all(&test_dir)?;

        // Read GLSL source
        let glsl_source = fs::read_to_string(glsl_file)?;

        // Copy GLSL file to test directory
        fs::write(test_dir.join(&test_name), &glsl_source)?;

        // Compile and transform
        let format = cli::parse_format(&args.format)?;
        let (mut module_before, mut module_after) = compile_and_transform(&glsl_source, format)?;

        // Process test (collect stats, write CLIF files, generate report)
        let summary = process_test(
            &test_name,
            &test_dir,
            &mut module_before,
            &mut module_after,
            &glsl_source,
            args.verbose,
        )?;

        test_summaries.push(summary);
    }

    // Generate overall report
    generate_reports(&report_dir, &test_summaries)?;

    if args.verbose {
        eprintln!("Report generated: {}", report_dir.display());
    }

    Ok(())
}

fn find_glsl_files(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("glsl") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn process_test(
    test_name: &str,
    test_dir: &Path,
    module_before: &mut lp_glsl_compiler::backend::module::gl_module::GlModule<
        cranelift_object::ObjectModule,
    >,
    module_after: &mut lp_glsl_compiler::backend::module::gl_module::GlModule<
        cranelift_object::ObjectModule,
    >,
    _glsl_source: &str,
    verbose: bool,
) -> Result<report::TestSummary> {
    // Write codegen files (CLIF, vcode, assembly) and get sizes
    let vcode_assembly_sizes =
        codegen::write_codegen_files(test_dir, module_before, module_after, verbose)?;

    // Collect statistics (will be updated to use vcode/assembly sizes)
    let stats_before = stats::collect_module_stats(module_before, &vcode_assembly_sizes)?;
    let stats_after = stats::collect_module_stats(module_after, &vcode_assembly_sizes)?;
    let delta = stats::calculate_deltas(&stats_before, &stats_after);

    // Generate test report
    let test_report = report::TestReport {
        name: test_name.to_string(),
        before: stats_before.clone(),
        after: stats_after.clone(),
        delta: delta.clone(),
        functions: stats::collect_function_reports(
            module_before,
            module_after,
            &vcode_assembly_sizes,
        )?,
    };
    report::generate_test_report(test_dir, &test_report)?;

    // Create summary for overall report
    Ok(report::TestSummary {
        name: test_name.to_string(),
        before: stats_before,
        after: stats_after,
        delta,
    })
}
