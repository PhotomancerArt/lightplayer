//! Run a project through today's live engine with the recording backend,
//! and say which fixture each recorded sample stream belongs to by finding
//! it, bit for bit, among the frozen copy's predictions.

use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use lp_gfx::{LpGraphics, ShaderEntrySpace};
use lpa_upgrade::v12::{V12Sampled, V12Space, V12Uniforms, sample_coordinates};
use lpc_engine::{ButtonService, EngineServices, ProjectLoader, RadioService};
use lpc_hardware::{HardwareSystem, HwRegistry, default_esp32c6_hardware_manifest};
use lpc_model::TreePath;
use lpfs::LpFsStd;
use lps_shared::LpsValueF32;

use super::project_tree::ProjectTree;
use super::recording_graphics::{Event, Log, RecordingGraphics, Swap};

/// Frames before the recorded one: past the compile window and the outputs'
/// settling.
const WARM_TICKS: usize = 8;

/// One recorded frame of a project: what every program was handed, and the
/// frame's error when it failed (a shader that runs out of fuel fails its
/// frame, and the batches before the failure are still recorded). `Err` is
/// a project the live engine will not load.
pub fn record_frame(dir: &Path) -> Result<(Log, Option<String>), String> {
    let log = Arc::new(Mutex::new(Log::default()));
    let mut rt = load(dir, &log)?;
    for _ in 0..WARM_TICKS {
        let _ = rt.tick(16);
    }
    log.lock().unwrap().events.clear();
    let failed = rt.tick(16).err().map(|e| format!("{e:?}"));
    Ok((std::mem::take(&mut *log.lock().unwrap()), failed))
}

/// Run a project with `swap` in place and keep each lamp's colour at each of
/// `times_ms` (from load, in 100 ms steps after the warm frames): what the
/// swapped program answers at the swapped coordinates.
pub fn swapped_frames(
    dir: &Path,
    swap: Swap,
    times_ms: &[u32],
) -> Result<Vec<Vec<Option<[u16; 4]>>>, String> {
    let lamps = swap.colors.len();
    let log = Arc::new(Mutex::new(Log {
        swap: Some(swap),
        ..Log::default()
    }));
    let mut rt = load(dir, &log)?;
    let mut elapsed = 0u32;
    for _ in 0..WARM_TICKS {
        let _ = rt.tick(16);
        elapsed += 16;
    }
    let mut frames = Vec::new();
    for &at in times_ms {
        while elapsed + 100 < at {
            let _ = rt.tick(100);
            elapsed += 100;
            log.lock().unwrap().events.clear();
        }
        if let Some(swap) = log.lock().unwrap().swap.as_mut() {
            swap.colors = vec![None; lamps];
        }
        let step = at.saturating_sub(elapsed).max(1);
        rt.tick(step)
            .map_err(|e| format!("frame at {at} ms: {e:?}"))?;
        elapsed += step;
        let mut log = log.lock().unwrap();
        log.events.clear();
        frames.push(
            log.swap
                .as_ref()
                .map(|s| s.colors.clone())
                .unwrap_or_default(),
        );
    }
    Ok(frames)
}

fn load(
    dir: &Path,
    log: &Arc<Mutex<Log>>,
) -> Result<lpc_engine::engine::LoadedProjectRuntime, String> {
    let graphics: Arc<dyn LpGraphics> = Arc::new(RecordingGraphics {
        inner: Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        )),
        log: Arc::clone(log),
    });
    let hardware = Rc::new(HardwareSystem::with_virtual_drivers(Rc::new(
        HwRegistry::new(default_esp32c6_hardware_manifest()),
    )));
    let button_service: Rc<dyn ButtonService> = hardware.clone();
    let radio_service: Rc<dyn RadioService> = hardware;
    let mut services = EngineServices::new(TreePath::parse("/project.show").expect("root path"));
    services.set_button_service(Some(button_service));
    services.set_radio_service(Some(radio_service));
    let fs = LpFsStd::new(dir.to_path_buf());
    let mut rt = ProjectLoader::load_from_root(&fs, services).map_err(|e| format!("{e:?}"))?;
    rt.engine_mut().set_graphics(Some(graphics));
    Ok(rt)
}

/// One recorded stream found among the predictions: which fixture, which
/// shader, how many lamps.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Matched {
    pub fixture: String,
    pub shader: String,
    pub lamps: usize,
}

/// What a recorded frame proved.
#[derive(Default)]
pub struct Attribution {
    pub matched: Vec<Matched>,
    /// Programs whose recorded batches could not be laid out as frozen
    /// predictions, in words.
    pub failures: Vec<String>,
    /// Programs that rendered a whole texture (no per-lamp coordinates).
    pub texture_renders: usize,
}

/// Lay every program's recorded batches out as the frozen copy's
/// predicted streams, bit for bit, with the uniforms bound for each.
pub fn attribute(tree: &ProjectTree, log: &Log) -> Attribution {
    let mut out = Attribution::default();
    for (program_id, program) in log.programs.iter().enumerate() {
        let space = match program.space {
            ShaderEntrySpace::TwoD => V12Space::TwoD,
            ShaderEntrySpace::OneD => V12Space::OneD,
        };
        // The batches, each with the uniforms bound when it ran.
        let mut bound: Option<&LpsValueF32> = None;
        let mut batches: Vec<(&[i32], Option<&LpsValueF32>)> = Vec::new();
        for event in &log.events {
            match event {
                Event::Bind { program, uniforms } if *program == program_id => {
                    bound = Some(uniforms)
                }
                Event::Batch { program, words } if *program == program_id => {
                    batches.push((words, bound))
                }
                Event::Render { program } if *program == program_id => out.texture_renders += 1,
                _ => {}
            }
        }
        if batches.is_empty() {
            continue;
        }
        let shaders = tree.shaders_with_source(&program.source);
        if shaders.is_empty() {
            out.failures.push(format!(
                "{}: a program sampled {} batches, and no shader file has its GLSL",
                tree.name,
                batches.len()
            ));
            continue;
        }
        let mut candidates: Vec<(String, String, V12Sampled)> = Vec::new();
        for shader in &shaders {
            for fixture in &tree.fixtures {
                let Some(v12) = fixture.v12() else { continue };
                if fixture.consume.is_some() {
                    continue;
                }
                if let Ok(sampled) = sample_coordinates(&v12, &shader.source)
                    && sampled.shader_space == space
                    && !sampled.words.is_empty()
                {
                    candidates.push((fixture.file.clone(), shader.file.clone(), sampled));
                }
            }
        }
        let mut path = Vec::new();
        if decompose(&batches, 0, &candidates, &mut path) {
            // A stream two fixtures predict alike (two copies of one strip)
            // proves both: credit every candidate equal to a matched one.
            for index in path {
                let (_, _, matched) = &candidates[index];
                for (fixture, shader, sampled) in &candidates {
                    if sampled == matched {
                        out.matched.push(Matched {
                            fixture: fixture.clone(),
                            shader: shader.clone(),
                            lamps: sampled.words.len() / sampled.lanes(),
                        });
                    }
                }
            }
            out.matched.dedup();
        } else {
            out.failures.push(format!(
                "{}: program of {} ({:?}): its {} recorded batches ({} words) are not the frozen \
                 copy's streams: {}",
                tree.name,
                shaders
                    .iter()
                    .map(|s| s.file.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                program.space,
                batches.len(),
                batches.iter().map(|(w, _)| w.len()).sum::<usize>(),
                nearest_miss(&batches, &candidates)
            ));
        }
    }
    out.matched.sort();
    out
}

/// Cover `batches[start..]` with candidates, each a run of whole batches
/// equal to its words and bound with its uniforms.
fn decompose(
    batches: &[(&[i32], Option<&LpsValueF32>)],
    start: usize,
    candidates: &[(String, String, V12Sampled)],
    path: &mut Vec<usize>,
) -> bool {
    if start == batches.len() {
        return true;
    }
    for (index, (_, _, sampled)) in candidates.iter().enumerate() {
        // A fixture may sample one shader more than once a frame (once per
        // output that reads it), so a candidate can be used again.
        if !uniforms_match(batches[start].1, &sampled.uniforms) {
            continue;
        }
        let want = &sampled.words;
        let mut at = 0usize;
        let mut end = start;
        let mut equal = true;
        while end < batches.len() && at < want.len() {
            let words = batches[end].0;
            if at + words.len() > want.len() || want[at..at + words.len()] != *words {
                equal = false;
                break;
            }
            at += words.len();
            end += 1;
        }
        if !equal || at != want.len() {
            continue;
        }
        path.push(index);
        if decompose(batches, end, candidates, path) {
            return true;
        }
        path.pop();
    }
    false
}

/// Do the bound uniforms say what the frozen copy says (`outputSize`, and
/// pattern space's three, or their absence)?
pub fn uniforms_match(bound: Option<&LpsValueF32>, want: &V12Uniforms) -> bool {
    let Some(block) = bound else {
        return false;
    };
    let vec2 = |name| match uniform(block, name) {
        Some(LpsValueF32::Vec2(v)) => Some(*v),
        _ => None,
    };
    let float = |name| match uniform(block, name) {
        Some(LpsValueF32::F32(v)) => Some(*v),
        _ => None,
    };
    if vec2("outputSize") != Some(want.output_size) {
        return false;
    }
    match &want.pattern {
        None => uniform(block, "patternExtent").is_none(),
        Some(p) => {
            vec2("patternExtent") == Some(p.extent)
                && float("patternPitch") == Some(p.pitch)
                && float("lampCount") == Some(p.lamp_count)
        }
    }
}

fn uniform<'a>(block: &'a LpsValueF32, name: &str) -> Option<&'a LpsValueF32> {
    let LpsValueF32::Struct { fields, .. } = block else {
        return None;
    };
    fields.iter().find(|(n, _)| n == name).map(|(_, v)| v)
}

/// The candidate whose first words agree longest with the first batch, and
/// where it first differs, for a failure message.
fn nearest_miss(
    batches: &[(&[i32], Option<&LpsValueF32>)],
    candidates: &[(String, String, V12Sampled)],
) -> String {
    let first = batches[0].0;
    let best = candidates
        .iter()
        .map(|(f, s, sampled)| {
            let agree = first
                .iter()
                .zip(&sampled.words)
                .take_while(|(a, b)| a == b)
                .count();
            (agree, f, s, sampled)
        })
        .max_by_key(|(agree, ..)| *agree);
    match best {
        None => "no fixture gives a prediction for it".to_string(),
        Some((agree, fixture, shader, sampled)) => format!(
            "nearest is {fixture} × {shader}: {agree} words agree, then recorded {:?} vs frozen \
             {:?}; uniforms {}",
            first.get(agree),
            sampled.words.get(agree),
            if uniforms_match(batches[0].1, &sampled.uniforms) {
                "agree"
            } else {
                "differ"
            }
        ),
    }
}
