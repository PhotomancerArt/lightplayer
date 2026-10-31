//! E16 research census (2026-10 RAM program): what the resolver's payload
//! cache holds on a catalog project, by query, and what a size cap keeps.
//!
//! Ignored by default — it prints, it does not assert anything a product
//! change should be held to. Run:
//!
//! ```text
//! cargo test -p lpc-engine --release --test payload_census -- --ignored --nocapture
//! ```
//!
//! `LP_CENSUS_PROJECT=<dir>` censuses another project directory instead of
//! `catalog/projects/playful-choker`. Host times are direction only.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use lpc_engine::{Engine, EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpc_registry::ProjectRegistry;
use lpfs::LpFsStd;

#[test]
#[ignore = "research census: prints, run by hand"]
fn payload_census_by_query_and_cap() {
    let dir = std::env::var("LP_CENSUS_PROJECT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace_dir().join("catalog/projects/playful-choker"));
    println!("project: {}", dir.display());

    let (mut engine, registry) = load(&dir);
    tick(&mut engine, &registry, 10);
    let mut census = engine.resolver().payload_census();
    census.sort_by(|a, b| b.2.cmp(&a.2));
    let total: usize = census.iter().map(|c| c.2).sum();
    println!("uncapped: {} payloads, {} B estimated", census.len(), total);
    for (key, structural, bytes) in census.iter().take(25) {
        let kind = if *structural { "structural" } else { "frame" };
        let key = key
            .as_ref()
            .map_or_else(|| "?".to_string(), |k| format!("{k:?}"));
        let key: String = key.chars().take(140).collect();
        println!("  {bytes:>6} B  {kind:<10}  {key}");
    }

    for cap in [
        None,
        Some(4096),
        Some(2048),
        Some(1024),
        Some(512),
        Some(256),
        Some(0),
    ] {
        let (mut engine, registry) = load(&dir);
        engine.resolver_mut().set_payload_cap(cap);
        tick(&mut engine, &registry, 20);
        let held: usize = engine.resolver().payload_census().iter().map(|c| c.2).sum();
        let started = Instant::now();
        let frames = 200;
        let mut uncached = 0u64;
        let mut hits = 0u64;
        for _ in 0..frames {
            engine.tick(&registry, 16).expect("tick");
            let counters = engine.resolver().frame_counters();
            uncached += counters.uncached_resolves as u64;
            hits += counters.cache_hits as u64;
        }
        let per_frame = started.elapsed().as_secs_f64() * 1e3 / frames as f64;
        println!(
            "cap {:>8}: held {held:>6} B, host {per_frame:.3} ms/frame, {:.1} uncached + {:.1} hits a frame",
            cap.map_or_else(|| "none".to_string(), |c| c.to_string()),
            uncached as f64 / frames as f64,
            hits as f64 / frames as f64,
        );
    }
    let (mut engine, registry) = load(&dir);
    engine.resolver_mut().set_retain_payloads(false);
    tick(&mut engine, &registry, 20);
    let started = Instant::now();
    for _ in 0..200 {
        engine.tick(&registry, 16).expect("tick");
    }
    println!(
        "payloads off: host {:.3} ms/frame",
        started.elapsed().as_secs_f64() * 1e3 / 200.0
    );
}

fn load(dir: &Path) -> (Engine, ProjectRegistry) {
    let fs = LpFsStd::new(dir.to_path_buf());
    let services = EngineServices::new(TreePath::parse("/census.show").expect("root path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services)
        .unwrap_or_else(|e| panic!("load {}: {e:?}", dir.display()));
    rt.engine_mut()
        .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
    let (mut engine, registry) = rt.into_parts();
    engine.resolver_mut().set_retain_payloads(true);
    tick(&mut engine, &registry, 3);
    (engine, registry)
}

fn tick(engine: &mut Engine, registry: &ProjectRegistry, frames: usize) {
    for _ in 0..frames {
        engine.tick(registry, 16).expect("tick");
    }
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("lpc-engine lives two levels under the workspace root")
        .to_path_buf()
}
