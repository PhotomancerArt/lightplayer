//! P5 render check: load every converted example through the real engine on
//! the CPU (lpvm) tier, tick to t = 5.0 s, and count the lit pixels of the
//! project's primary visual.
//!
//! The lps-probe A/B runs on the f32 LPIR interpreter with hand-supplied
//! uniforms. This one runs the whole pipeline — project load, phasor slot
//! evaluation against a real ClockNode and timebase store, Q16.16 shader
//! execution — and answers the only question that matters after a sweep:
//! did anything go black? Disposable (P9).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lpc_engine::products::visual::RenderTextureRequest;
use lpc_engine::{ButtonService, EngineServices, ProjectLoader, RadioService};
use lpc_hardware::{HardwareSystem, HwRegistry, default_esp32c6_hardware_manifest};
use lpc_model::TreePath;
use lpfs::LpFsStd;
use lps_shared::TextureStorageFormat;

/// (ledger body key, project dir, bus channel)
const PROJECTS: &[(&str, &str)] = &[
    ("fast", "examples/fast"),
    ("fiber-headband", "examples/fiber-headband"),
    ("rocaille", "examples/rocaille"),
    ("quad-strips", "projects/test/quad-strips"),
    ("penta-strands", "projects/test/penta-strands-v3"),
    ("plasma", "examples/plasma"),
    ("smoke-project", "lp-fw/fw-browser/www/smoke-project"),
    ("basic2", "examples/basic2"),
    ("basic", "examples/basic"),
    ("perf", "examples/perf/baseline"),
    ("button-idle", "examples/button-playlist"),
    ("fyeah-attract", "examples/fyeah-button"),
    ("fyeah-idle", "examples/fyeah-sign"),
    ("fyeah-idle-plain", "projects/test/fyeah-sign"),
    ("fluid-compute", "examples/fluid"),
    ("meteor-sim", "examples/meteor"),
    ("events-a", "examples/events"),
];

const TICK_MS: u32 = 25;
const TICKS: u32 = 200; // 5.000 s

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("worktree root")
        .to_path_buf()
}

/// (lit fraction at t=5 s, pixel count, mean |Δ| between the t=1 s and
/// t=5 s frames in 16-bit units)
fn lit_fraction(dir: &str) -> Result<(f32, usize, f64), String> {
    let fs = LpFsStd::new(workspace().join(dir));
    let mut services = EngineServices::new(TreePath::parse("/check.show").expect("root path"));
    // Button/radio-driven examples (playlists) refuse to tick without these.
    let hardware = std::rc::Rc::new(HardwareSystem::with_virtual_drivers(std::rc::Rc::new(
        HwRegistry::new(default_esp32c6_hardware_manifest()),
    )));
    let button: std::rc::Rc<dyn ButtonService> = hardware.clone();
    let radio: std::rc::Rc<dyn RadioService> = hardware;
    services.set_button_service(Some(button));
    services.set_radio_service(Some(radio));
    let mut rt = ProjectLoader::load_from_root(&fs, services).map_err(|e| format!("load: {e}"))?;
    rt.engine_mut()
        .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
    // t = 1 s, then t = 5 s: a still frame proves nothing about a timebase.
    let mut early: Vec<u8> = Vec::new();
    for tick in 1..=TICKS {
        rt.tick(TICK_MS).map_err(|e| format!("tick: {e}"))?;
        if tick * TICK_MS == 1000 {
            early = frame(&mut rt)?;
        }
    }
    let bytes = frame(&mut rt)?;
    let motion = if early.len() == bytes.len() && !bytes.is_empty() {
        let n = bytes.len() / 2;
        (0..n)
            .map(|i| {
                let a = u16::from_le_bytes([early[i * 2], early[i * 2 + 1]]) as f64;
                let b = u16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]) as f64;
                (a - b).abs()
            })
            .sum::<f64>()
            / n as f64
    } else {
        0.0
    };
    let px = bytes.len() / 8;
    let mut lit = 0usize;
    for i in 0..px {
        let rgb = (0..3).map(|c| {
            u16::from_le_bytes([bytes[i * 8 + c * 2], bytes[i * 8 + c * 2 + 1]])
        });
        if rgb.into_iter().any(|v| v > 256) {
            lit += 1;
        }
    }
    Ok((lit as f32 / px as f32, px, motion))
}

fn frame(rt: &mut lpc_engine::engine::LoadedProjectRuntime) -> Result<Vec<u8>, String> {
    let (engine, registry) = rt.read_parts();
    let product = engine
        .resolve_bus_visual_product(registry, "visual.out")
        .map_err(|e| format!("resolve visual.out: {e}"))?;
    let texture = engine
        .render_texture_product(
            registry,
            product,
            &RenderTextureRequest {
                width: 16,
                height: 16,
                format: TextureStorageFormat::Rgba16Unorm,
                time_seconds: 0.0,
            },
        )
        .map_err(|e| format!("render: {e}"))?;
    Ok(texture.try_raw_bytes().ok_or("no CPU bytes")?.to_vec())
}

fn main() {
    let mut out = serde_json::Map::new();
    let mut failures = 0;
    for (key, dir) in PROJECTS {
        match lit_fraction(dir) {
            Ok((frac, px, motion)) => {
                let ok = frac > 0.0 && motion > 0.0;
                if !ok {
                    failures += 1;
                }
                println!(
                    "{:<18} {:<40} lit {:>6.1}% of {px} px, 1s->5s motion {:>8.1}  [{}]",
                    key,
                    dir,
                    frac * 100.0,
                    motion,
                    if ok { "animating" } else { "STILL/BLACK" }
                );
                out.insert(
                    key.to_string(),
                    serde_json::json!({
                        "project": dir,
                        "lit_fraction": frac,
                        "motion_1s_to_5s": motion,
                        "ok": ok
                    }),
                );
            }
            Err(e) => {
                failures += 1;
                println!("{key:<18} {dir:<40} ERROR {e}");
                out.insert(
                    key.to_string(),
                    serde_json::json!({"project": dir, "error": e, "ok": false}),
                );
            }
        }
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("render-check.json");
    serde_json::to_writer_pretty(std::fs::File::create(path).unwrap(), &out).unwrap();
    if failures > 0 {
        eprintln!("{failures} project(s) failed the render check");
        std::process::exit(1);
    }
}
