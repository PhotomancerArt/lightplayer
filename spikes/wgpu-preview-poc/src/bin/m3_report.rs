//! Render the whole corpus on both paths, dump PNGs, and print the
//! divergence/timing tables as markdown (pasted into m3-report.md).
//!
//! Run: `cargo run -p wgpu-preview-poc --release --bin m3_report`
//! PNGs land in `target/wgpu-preview-poc/` (gitignored).

use std::path::PathBuf;

use wgpu_preview_poc::corpus::CORPUS;
use wgpu_preview_poc::diff::{
    count_non_finite, diff_frames, quantize_gpu_frame, write_frame_png, write_side_by_side_grid,
};
use wgpu_preview_poc::glsl_to_wgsl::{glsl_to_wgsl, uniform_values};
use wgpu_preview_poc::gpu::GpuFrameRenderer;
use wgpu_preview_poc::reference::ReferenceRenderer;

const WIDTH: u32 = 128;
const HEIGHT: u32 = 128;
const TIMESTAMPS: &[f32] = &[0.0, 2.5, 5.0];

fn main() {
    let out_dir: PathBuf =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/wgpu-preview-poc");
    let Some(gpu) = GpuFrameRenderer::new() else {
        eprintln!("no GPU adapter available; nothing to do");
        std::process::exit(1);
    };
    println!(
        "adapter: {} ({:?}, {:?})\n",
        gpu.adapter_info.name, gpu.adapter_info.backend, gpu.adapter_info.device_type
    );
    let reference = ReferenceRenderer::new().expect("reference renderer");

    println!("## Corpus\n");
    println!("| shader | source | exercises |");
    println!("|---|---|---|");
    for shader in CORPUS {
        println!(
            "| {} | `{}` | {} |",
            shader.name, shader.path, shader.exercises
        );
    }

    let mut divergence_rows = Vec::new();
    let mut timing_rows = Vec::new();

    for shader in CORPUS {
        // rocaille's 81-iteration nested loops exhaust the wasm runtime's
        // fixed 64M-unit per-call fuel budget at 128² and 64² (finding for
        // the report); render it at 32² so the reference completes.
        let (width, height) = if shader.name == "rocaille" {
            (32u32, 32u32)
        } else {
            (WIDTH, HEIGHT)
        };
        let translated = glsl_to_wgsl(shader).unwrap_or_else(|e| panic!("{}: {e}", shader.name));
        std::fs::create_dir_all(&out_dir).expect("create out dir");
        std::fs::write(
            out_dir.join(format!("{}_assembled.glsl", shader.name)),
            &translated.assembled_glsl,
        )
        .expect("write assembled glsl");
        std::fs::write(
            out_dir.join(format!("{}.wgsl", shader.name)),
            &translated.wgsl,
        )
        .expect("write wgsl");
        let pipeline = gpu.create_pipeline(&translated.wgsl, &translated.uniforms);
        let compiled_ref = reference.compile(shader).expect(shader.name);

        timing_rows.push(format!(
            "| {} | {:.2} | {:.2} | {:.2} | {:.2} | {:.2} | {:.1} |",
            shader.name,
            ms(translated.timings.parse),
            ms(translated.timings.validate),
            ms(translated.timings.wgsl_out),
            ms(pipeline.timings.create_shader_module),
            ms(pipeline.timings.create_pipeline),
            ms(compiled_ref.compile_time),
        ));

        let mut grid_frames = Vec::new();
        for &t in TIMESTAMPS {
            let ref_frame = reference
                .render(shader, &compiled_ref, width, height, t)
                .expect(shader.name);
            let values =
                uniform_values(shader, &translated.uniforms, width, height, t).expect(shader.name);
            let raw_gpu = gpu.render(&pipeline, &values, width, height);
            let non_finite = count_non_finite(&raw_gpu);
            let gpu_frame = quantize_gpu_frame(&raw_gpu);

            let stats = diff_frames(&ref_frame, &gpu_frame);
            divergence_rows.push(format!(
                "| {} ({width}²) | {t:.1} | {:.3} | {:.1} | {:.2}% | {} |",
                shader.name,
                stats.mean_8bit(),
                stats.max_8bit(),
                stats.frac_over_8bit_8 * 100.0,
                non_finite,
            ));

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

    println!("\n## Divergence (Q32 wasm reference vs GPU f32, {WIDTH}x{HEIGHT})\n");
    println!(
        "| shader | t (s) | mean |Δ| (8-bit) | max |Δ| (8-bit) | px > 8/255 | non-finite lanes |"
    );
    println!("|---|---|---|---|---|---|");
    for row in &divergence_rows {
        println!("{row}");
    }

    println!("\n## Pipeline timings (ms)\n");
    println!(
        "| shader | naga parse | naga validate | wgsl-out | wgpu module | wgpu pipeline | Q32 wasm compile |"
    );
    println!("|---|---|---|---|---|---|---|");
    for row in &timing_rows {
        println!("{row}");
    }

    println!("\nPNGs: {}", out_dir.display());
}

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}
