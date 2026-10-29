//! The frozen format-12 sampling copy (`lpa_upgrade::v12`) is today's live
//! engine, bit for bit, on every project in the repo.
//!
//! Each project is loaded and ticked by the live engine over the real CPU
//! graphics tier behind a recorder (`sampling_support::recording_graphics`):
//! every coordinate window a shader program samples, and every uniform block
//! bound for it, is kept. The recorded frame must lay out, batch by batch,
//! as the frozen copy's predicted streams — each one fixture's whole lamp
//! list for one shader, every word equal, its `outputSize` and pattern-space
//! uniforms equal. A playlist is run once per entry (a scratch copy starting
//! on that entry), so every entry's shader is sampled.
//!
//! The sample-parity test of the format step stands on this copy, so this
//! is what makes that test mean anything (plan PD11).
//!
//! ```bash
//! cargo test -p lpa-upgrade --test v12_sampling_live -- --nocapture
//! ```

mod sampling_support;

use std::collections::BTreeSet;

use lpa_upgrade::v12::V12Sampling;
use sampling_support::live_streams::{Matched, attribute, record_frame};
use sampling_support::project_tree::{ProjectTree, repo_projects, workspace_dir};
use sampling_support::scratch_copy::ScratchCopy;

/// Every sample stream today's live engine hands a shader, in every repo
/// project and every playlist entry, is the frozen copy's prediction for one
/// fixture and that shader, bit for bit, with the same uniforms.
#[test]
fn the_frozen_copy_samples_what_the_live_engine_samples_on_every_repo_project() {
    let workspace = workspace_dir();
    let mut failures = Vec::new();
    let mut matched: BTreeSet<(String, Matched)> = BTreeSet::new();
    let mut runs = 0usize;
    let mut texture_renders = 0usize;
    let mut unmatched = Vec::new();
    let projects = repo_projects(&workspace);
    for dir in &projects {
        let tree = ProjectTree::read(&workspace, dir);
        let mut dirs = vec![(dir.clone(), None)];
        for playlist in &tree.playlists {
            for entry in &playlist.entries {
                let Ok(key) = entry.parse::<u64>() else {
                    continue;
                };
                if Some(key) != playlist.idle_entry {
                    let copy = ScratchCopy::with_idle_entry(dir, &playlist.file, key);
                    dirs.push((copy.dir.clone(), Some(copy)));
                }
            }
        }
        for (run_dir, _keep) in &dirs {
            runs += 1;
            let log = match record_frame(run_dir) {
                Ok((log, None)) => log,
                Ok((_, Some(why))) if why.contains("fuel exhausted") => {
                    // A shader that never finishes (the fault demo) fails its
                    // frame part-way through a stream: nothing to compare.
                    unmatched.push(format!("{}: its frame fails: {why}", tree.name));
                    continue;
                }
                Ok((_, Some(why))) => {
                    failures.push(format!("{}: the live frame failed: {why}", tree.name));
                    continue;
                }
                Err(why) => {
                    failures.push(format!("{}: the live engine refused it: {why}", tree.name));
                    continue;
                }
            };
            let attribution = attribute(&tree, &log);
            texture_renders += attribution.texture_renders;
            failures.extend(attribution.failures);
            if std::env::var_os("LP_V12_DEBUG").is_some() {
                eprintln!(
                    "{} run {}: {} programs; matched {:?}",
                    tree.name,
                    run_dir.display(),
                    log.programs.len(),
                    attribution
                        .matched
                        .iter()
                        .map(|m| (&m.fixture, &m.shader))
                        .collect::<Vec<_>>()
                );
            }
            for m in attribution.matched {
                matched.insert((tree.name.clone(), m));
            }
        }
        for fixture in &tree.fixtures {
            let seen = matched
                .iter()
                .any(|(p, m)| p == &tree.name && m.fixture == fixture.file);
            if !seen {
                let why = match (&fixture.lamps, fixture.sampling) {
                    (Err(e), _) => format!("its map2d: {e}"),
                    (_, V12Sampling::TextureArea) => "texture-area sampling".to_string(),
                    (Ok(l), _) if l.positions.is_empty() => "no lamps".to_string(),
                    _ if fixture.diagnostic.is_some() => format!(
                        "diagnostic mode {}: its lamps show a test pattern",
                        fixture.diagnostic.as_deref().unwrap_or("")
                    ),
                    _ if !tree.fluids.is_empty() => "fed by a fluid, not a shader".to_string(),
                    _ => "no shader sampled it".to_string(),
                };
                unmatched.push(format!("{} {}: {why}", tree.name, fixture.file));
            }
        }
    }

    let lamps: usize = matched.iter().map(|(_, m)| m.lamps).sum();
    let fixtures: BTreeSet<_> = matched.iter().map(|(p, m)| (p, &m.fixture)).collect();
    eprintln!(
        "{} projects, {runs} runs: {} fixture × shader streams equal bit for bit ({} fixtures, \
         {lamps} lamp coordinates), {texture_renders} whole-texture renders not compared",
        projects.len(),
        matched.len(),
        fixtures.len()
    );
    for line in &unmatched {
        eprintln!("  not sampled per lamp: {line}");
    }
    assert!(
        failures.is_empty(),
        "the frozen copy and the live engine disagree:\n{}",
        failures.join("\n")
    );
    assert!(
        projects.len() > 70,
        "only {} projects found",
        projects.len()
    );
    assert!(
        matched.len() > 100,
        "only {} streams compared",
        matched.len()
    );
    assert!(
        matched
            .iter()
            .any(|(p, m)| p == "catalog/projects/playful-choker" && m.fixture == "fixture.json"),
        "the choker was compared"
    );
}
