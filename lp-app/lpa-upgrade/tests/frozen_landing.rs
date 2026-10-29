//! The frozen format-11 landing agrees with the live engine, lamp for lamp,
//! on every upgrade-corpus project carried by the chain to format 11.
//!
//! The "before" side is the landing goldens under `tests/landing/`, recorded
//! from the live engine at project format 11. [`lpa_upgrade::landing`]
//! computes the same answer from the files alone — the arithmetic the
//! v11→v12 step is built on. The two share no code, so a bug in the frozen
//! copy cannot hide behind itself. Until the bump this walked every project
//! in the repo; the repo is at format 12 now, so it walks the corpus, each
//! project carried by the chain up to 11 (stopping before `v11_to_v12`).
//!
//! ```bash
//! cargo test -p lpa-upgrade --test frozen_landing
//! ```

use std::collections::BTreeMap;

use lpa_upgrade::{FixtureLayout, Landing, SegmentSource, landing};

mod common;

use common::{corpus_at_11, workspace_dir};

/// For every corpus project with a landing golden, carried to format 11,
/// the frozen landing lands every lamp exactly where the engine did.
#[test]
fn the_frozen_landing_lands_every_lamp_where_the_engine_did() {
    let workspace = workspace_dir();
    let mut compared = 0usize;
    let mut skipped = Vec::new();
    let mut failures = Vec::new();
    let corpus = corpus_at_11(&workspace);
    let total = corpus.len();
    for (project, files, golden) in corpus {
        if let Some(why) = golden.refused {
            skipped.push(format!("{project}: the engine refused it ({why})"));
            continue;
        }
        let landed = match landing(&files) {
            Ok(landed) => landed,
            Err(error) => {
                failures.push(format!("{project}: the frozen landing refused it: {error}"));
                continue;
            }
        };
        compared += 1;
        let frozen = lamp_landings(&landed);
        // A map2d already at format 6 is not the frozen landing's to read
        // (the step passes it through byte for byte); the engine's parity
        // check covers it.
        let mut recorded = golden.fixtures;
        for fixture in &landed.fixtures {
            if fixture.layout == FixtureLayout::AlreadyFormat6 {
                recorded.remove(&fixture.node);
                skipped.push(format!(
                    "{project} {}: its map2d is already format 6",
                    fixture.node
                ));
            }
        }
        if let Some(difference) = first_difference(&recorded, &frozen) {
            failures.push(format!("{project}: {difference}"));
        }
    }
    eprintln!("frozen landing: compared {compared} projects");
    for skip in &skipped {
        eprintln!("  skipped {skip}");
    }
    assert!(
        failures.is_empty(),
        "the frozen landing disagrees with the engine:\n{}",
        failures.join("\n")
    );
    assert_eq!(
        compared + skipped.iter().filter(|s| s.contains("refused")).count(),
        total,
        "every corpus golden is compared or refused"
    );
    assert!(compared >= 25, "only {compared} projects were compared");
}

/// No corpus segment is cut by a port boundary or a row. The one cut the
/// repo had, quad-wire-oracle's 64-lamp grid over four 16-lamp ports, was
/// split by hand into four 8 × 2 grids, one per port (M3 P4, lean Q70); the
/// cut refusal is tested on the old grid in `tests/refusals.rs`.
#[test]
fn no_corpus_segment_is_cut() {
    let workspace = workspace_dir();
    let mut cuts = Vec::new();
    for (project, files, golden) in corpus_at_11(&workspace) {
        if golden.refused.is_some() {
            continue;
        }
        let Ok(landed) = landing(&files) else {
            continue;
        };
        for fixture in &landed.fixtures {
            let FixtureLayout::Segments(segments) = &fixture.layout else {
                continue;
            };
            for segment in segments {
                if let Landing::Cut { why, pieces } = &segment.landing {
                    cuts.push(format!(
                        "{project} {} {}: {why} ({} pieces)",
                        fixture.node,
                        source_name(&segment.source),
                        pieces.len()
                    ));
                }
            }
        }
    }
    assert_eq!(cuts, Vec::<String>::new());
}

/// `(output, port, channel)` of one lamp on one wire.
type Place = common::Place;

// --- the frozen side --------------------------------------------------------

/// The frozen landing, lamp by lamp, in the golden's shape.
fn lamp_landings(landed: &lpa_upgrade::ProjectLanding) -> BTreeMap<String, Vec<Vec<Place>>> {
    let mut fixtures = BTreeMap::new();
    for fixture in &landed.fixtures {
        let mut places: Vec<Vec<Place>> = vec![Vec::new(); fixture.lamps as usize];
        if let FixtureLayout::Segments(segments) = &fixture.layout {
            for segment in segments {
                match &segment.landing {
                    Landing::Landed {
                        output,
                        port,
                        first_channel,
                        reversed,
                        rotation,
                    } => {
                        let n = segment.lamps;
                        for j in 0..n {
                            let flipped = if *reversed { n - 1 - j } else { j };
                            let slot = (flipped + rotation) % n;
                            places[(segment.first_lamp + j) as usize].push((
                                output.clone(),
                                *port,
                                first_channel + 3 * slot,
                            ));
                        }
                    }
                    Landing::Dark { .. } => {}
                    Landing::Cut { pieces, .. } => {
                        for piece in pieces {
                            for k in 0..piece.lamps {
                                let channel = i64::from(piece.first_channel)
                                    + i64::from(piece.step) * i64::from(k);
                                places[(piece.first_lamp + k) as usize].push((
                                    piece.output.clone(),
                                    piece.port,
                                    channel as u32,
                                ));
                            }
                        }
                    }
                }
            }
        }
        if fixture.layout != FixtureLayout::AlreadyFormat6 {
            fixtures.insert(fixture.node.clone(), places);
        }
    }
    fixtures
}

/// The first lamp the two sides disagree on, in words.
fn first_difference(
    golden: &BTreeMap<String, Vec<Vec<Place>>>,
    frozen: &BTreeMap<String, Vec<Vec<Place>>>,
) -> Option<String> {
    let recorded: Vec<&String> = golden.keys().collect();
    let computed: Vec<&String> = frozen.keys().collect();
    if recorded != computed {
        return Some(format!(
            "fixtures: the engine has {recorded:?}, the frozen landing {computed:?}"
        ));
    }
    for (node, lamps) in golden {
        let other = &frozen[node];
        if lamps.len() != other.len() {
            return Some(format!(
                "{node}: the engine has {} lamps, the frozen landing {}",
                lamps.len(),
                other.len()
            ));
        }
        for (lamp, (engine, computed)) in lamps.iter().zip(other).enumerate() {
            if engine != computed {
                return Some(format!(
                    "{node} lamp {lamp}: the engine lands it at {engine:?}, the frozen landing \
                     at {computed:?}"
                ));
            }
        }
    }
    None
}

fn source_name(source: &SegmentSource) -> String {
    match source {
        SegmentSource::Object {
            index,
            id,
            name,
            instances,
        } => {
            let mut text = id.clone().unwrap_or_else(|| {
                if name.is_empty() {
                    format!("object {index}")
                } else {
                    name.clone()
                }
            });
            for instance in instances {
                text.push_str(&format!("@{instance}"));
            }
            text
        }
        SegmentSource::PointList { key, .. } => format!("path {key}"),
    }
}
