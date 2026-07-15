//! P5 corpus parity: render the spike corpus through `GpuGraphics` at the
//! spike's sizes/timestamps and diff against the authoritative `wasm.q32`
//! reference — the numbers must **hold or beat** m3-report.md.
//!
//! PNGs (per-frame + side-by-side grids) land in
//! `target/lp-gfx-wgpu-parity/` (gitignored) for the review gate.
//! Adapter-gated: skips cleanly without a GPU.

mod util;

use std::path::PathBuf;
use std::time::Instant;

use lp_gfx::{LpGraphics, ShaderCompileOptions, ShaderSemantics};
use util::corpus::{CORPUS, CorpusShader};
use util::diff::{DiffStats, diff_frames, write_frame_png, write_side_by_side_grid};
use util::reference::{ReferenceRenderer, corpus_uniforms};

const TIMESTAMPS: &[f32] = &[0.0, 2.5, 5.0];

/// Hold-or-beat bounds from m3-report.md (worst per-timestamp mean |Δ| in
/// 8-bit units). rocaille's divergence is structural (Q32 saturation by
/// design); its bound reflects the spike's observed ≈10.9 with small
/// headroom rather than a preview-quality claim.
fn mean_8bit_bound(name: &str) -> f64 {
    match name {
        "basic" => 2.5,
        "basic2" => 21.0,
        "fyeah_idle" => 1.7,
        "fyeah_blast" => 3.3,
        "rocaille" => 12.0,
        other => panic!("no bound for corpus shader {other}"),
    }
}

fn frame_size(shader: &CorpusShader) -> (u32, u32) {
    // rocaille's 81-iteration loops exhaust the wasm runtime's per-call
    // fuel budget above 32² (m3 finding); the reference must complete.
    if shader.name == "rocaille" {
        (32, 32)
    } else {
        (128, 128)
    }
}

#[test]
fn corpus_parity_holds_or_beats_m3() {
    let Some(graphics) = util::test_graphics() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };
    let reference = ReferenceRenderer::new().expect("reference renderer");
    let out_dir: PathBuf =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/lp-gfx-wgpu-parity");
    let options = ShaderCompileOptions {
        semantics: ShaderSemantics::F32Gpu,
        ..Default::default()
    };

    println!("| shader | t (s) | mean |D| (8-bit) | max |D| (8-bit) | px > 8/255 | bound (mean) |");
    println!("|---|---|---|---|---|---|");
    let mut timing_rows = Vec::new();
    let mut failures = Vec::new();

    for shader in CORPUS {
        let (width, height) = frame_size(shader);

        let gpu_compile_start = Instant::now();
        let mut compiled_gpu = graphics
            .compile_shader(shader.source, &options)
            .unwrap_or_else(|e| panic!("{}: gpu compile: {e}", shader.name));
        let gpu_compile = gpu_compile_start.elapsed();

        let compiled_ref = reference.compile(shader).expect(shader.name);
        timing_rows.push(format!(
            "| {} | {:.1} | {:.1} |",
            shader.name,
            gpu_compile.as_secs_f64() * 1000.0,
            compiled_ref.compile_time.as_secs_f64() * 1000.0,
        ));

        let bound = mean_8bit_bound(shader.name);
        let mut grid_frames = Vec::new();
        for &t in TIMESTAMPS {
            let ref_frame = reference
                .render(shader, &compiled_ref, width, height, t)
                .expect(shader.name);

            let mut target = graphics
                .create_render_target(width, height)
                .expect("render target");
            compiled_gpu
                .render(&mut target, &corpus_uniforms(shader, width, height, t))
                .unwrap_or_else(|e| panic!("{}: gpu render: {e}", shader.name));
            let gpu_frame: Vec<u16> = graphics
                .read_back(&target)
                .expect("read back")
                .bytes()
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect();

            let stats: DiffStats = diff_frames(&ref_frame, &gpu_frame);
            println!(
                "| {} ({width}^2) | {t:.1} | {:.3} | {:.1} | {:.2}% | <= {bound} |",
                shader.name,
                stats.mean_8bit(),
                stats.max_8bit(),
                stats.frac_over_8bit_8 * 100.0,
            );
            if stats.mean_8bit() > bound {
                failures.push(format!(
                    "{} t={t}: mean {:.3} exceeds the m3 hold-or-beat bound {bound}",
                    shader.name,
                    stats.mean_8bit()
                ));
            }

            write_frame_png(
                &out_dir.join(format!("{}_t{t:.1}_ref.png", shader.name)),
                width,
                height,
                &ref_frame,
            )
            .expect("write ref png");
            write_frame_png(
                &out_dir.join(format!("{}_t{t:.1}_gpu.png", shader.name)),
                width,
                height,
                &gpu_frame,
            )
            .expect("write gpu png");
            grid_frames.push((ref_frame, gpu_frame));
        }
        write_side_by_side_grid(
            &out_dir.join(format!("{}_grid.png", shader.name)),
            width,
            height,
            &grid_frames,
        )
        .expect("write grid png");
    }

    println!("\n| shader | gpu compile (ms, assemble->pipeline) | q32 wasm compile (ms) |");
    println!("|---|---|---|");
    for row in &timing_rows {
        println!("{row}");
    }
    println!("\nPNGs: {}", out_dir.display());

    assert!(
        failures.is_empty(),
        "parity bounds missed (finding for the review gate, not a threshold to adjust):\n{}",
        failures.join("\n")
    );
}
