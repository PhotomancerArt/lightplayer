//! How does the GLSL compiler's peak working set scale with shader size?
//!
//! Context: the flash-budget ADR argues that shaders in the 17-50 KB range are
//! already unreachable because "a 4 KB shader needs ~65 KB of compile working
//! set", so shrinking the JIT code region cannot be the binding constraint for
//! them. PR #284 cuts ChunkedVec's chunk allocations, so that claim needs
//! re-checking: if the transient now scales far enough down, a 17 KB shader
//! might become compilable and the region size WOULD bind.
//!
//! This measures peak live heap during a real `lps_glsl::compile`, on the host.
//!
//! Caveat, stated up front: this is a 64-bit host, so every pointer-bearing
//! type is larger here than on the 32-bit device (`HirExpr` is 96 B there).
//! Absolute bytes are therefore an OVER-estimate of the device figure. The
//! scaling *slope* is the transferable part, and the real shader is measured
//! alongside the synthetic sweep so the two can be checked against each other.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static LARGEST: AtomicUsize = AtomicUsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            let now = LIVE.fetch_add(l.size(), Ordering::Relaxed) + l.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
            LARGEST.fetch_max(l.size(), Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }
}

#[global_allocator]
static A: Counting = Counting;

fn measure(source: &str) -> Option<(usize, usize)> {
    LIVE.store(0, Ordering::Relaxed);
    PEAK.store(0, Ordering::Relaxed);
    LARGEST.store(0, Ordering::Relaxed);
    let opts = lps_glsl::CompileOptions::default();
    match lps_glsl::compile(source, &opts) {
        Ok(out) => {
            let peak = PEAK.load(Ordering::Relaxed);
            let largest = LARGEST.load(Ordering::Relaxed);
            drop(out);
            Some((peak, largest))
        }
        Err(d) => {
            eprintln!("    compile failed: {d:?}");
            None
        }
    }
}

/// Grow a shader by repeating expression-heavy statements inside the entry
/// point, so the HIR expression count scales with source size the way a real
/// bigger shader's would. Shape follows `examples/basic/shader.glsl`: a
/// `vec4 render(vec2)` entry point with `layout(binding = N) uniform` inputs.
fn synthetic(repeats: usize) -> String {
    let mut s = String::from(
        "layout(binding = 0) uniform vec2 outputSize;\n         layout(binding = 1) uniform float time;\n\n         vec4 render(vec2 pos) {\n         \x20 vec3 c = vec3(0.0);\n         \x20 float t = time + pos.x / outputSize.x;\n",
    );
    for i in 0..repeats {
        s.push_str(&format!(
            "  c += vec3(sin(t * {i}.0 + 1.0), cos(t * {i}.0 + 2.0), sin(t * {i}.0 * 0.5));\n  \
             t = fract(t * 1.37 + dot(c, vec3(0.3, 0.6, 0.1)) + {i}.0);\n"
        ));
    }
    s.push_str("  return vec4(c, 1.0);\n}\n");
    s
}

/// Which stage owns the largest single allocation? `lex` is public, so the
/// lexer's token vector can be measured on its own and subtracted out.
fn measure_lex(source: &str) -> (usize, usize) {
    LIVE.store(0, Ordering::Relaxed);
    PEAK.store(0, Ordering::Relaxed);
    LARGEST.store(0, Ordering::Relaxed);
    let toks = lps_glsl::lex(source);
    let peak = PEAK.load(Ordering::Relaxed);
    let largest = LARGEST.load(Ordering::Relaxed);
    drop(toks);
    (peak, largest)
}

fn main() {
    println!("host pointer width: {} bits\n", usize::BITS);

    let real = std::fs::read_to_string("../../examples/basic/shader.glsl")
        .expect("examples/basic/shader.glsl");
    println!("=== the real shader ===");
    match measure(&real) {
        Some((peak, largest)) => println!(
            "examples/basic/shader.glsl: {} B of GLSL -> peak {} B, largest single alloc {} B  ({:.1}x source)",
            real.len(),
            peak,
            largest,
            peak as f64 / real.len() as f64
        ),
        None => println!("examples/basic/shader.glsl: did not compile"),
    }

    let (lex_peak, lex_largest) = measure_lex(&real);
    println!(
        "  lexing alone:            peak {lex_peak} B, largest single alloc {lex_largest} B"
    );

    println!("\n=== synthetic sweep (establishes the slope) ===");
    println!("{:>8}  {:>10}  {:>10}  {:>8}", "GLSL B", "peak B", "largest B", "peak/src");
    let mut pts: Vec<(usize, usize)> = Vec::new();
    for repeats in [4usize, 8, 16, 32, 64, 128, 256] {
        let src = synthetic(repeats);
        if let Some((peak, largest)) = measure(&src) {
            println!(
                "{:>8}  {:>10}  {:>10}  {:>8.1}",
                src.len(),
                peak,
                largest,
                peak as f64 / src.len() as f64
            );
            pts.push((src.len(), peak));
        }
    }

    if pts.len() >= 2 {
        let (x0, y0) = pts[pts.len() / 2];
        let (x1, y1) = pts[pts.len() - 1];
        let slope = (y1 as f64 - y0 as f64) / (x1 as f64 - x0 as f64);
        println!("\nslope over the top half: {slope:.2} B of peak per B of GLSL");
        for target in [4_092usize, 17_000, 50_000] {
            println!(
                "  extrapolated peak at {:>6} B of GLSL: {:>9.0} B   (110 KiB arena = 112,640; +64 KiB = 178,176)",
                target,
                y1 as f64 + slope * (target as f64 - x1 as f64)
            );
        }
    }
}
