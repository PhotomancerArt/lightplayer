//! Where every lamp of every fixture of a format-11 project lands: the
//! engine's composition, frozen.
//!
//! Frozen from `lp-core/lpc-engine` at origin/main b9f862fb2 — the
//! fixture's patch resolution (`FixtureNode::ensure_patch_resolved`), the
//! engine's routing (`Engine::control_patch_placement`), the output's plan
//! (`plan_fragments`) and the wires' port slicing (`planned_wires`,
//! `remainder_offender`):
//!
//! 1. Each fixture resolves its patch on its own. An empty document is no
//!    patch (auto-flow, or nothing under `flow: manual`); a document the
//!    kernel refuses falls back the same way; a document the engine cannot
//!    read at all (not JSON, or not a patch it knows) is attached with no
//!    placement, and the fixture auto-flows (`resolve_fixture_patch`; DD8:
//!    the upgrade keeps that and says so); a row naming an output no
//!    output is named is dropped, its lamps unplaced — and a fixture left
//!    with no runs at all is treated as unpatched.
//! 2. Each output, for each fixture it reads (provider order): an unpatched
//!    fixture auto-flows on the default output only, unless manual; a
//!    patched one contributes the runs naming this output (or none, for the
//!    default output).
//! 3. The output places every run where it says, then the auto-flowed
//!    fixtures one after another, starting past the highest anchored end.
//! 4. The output's buffer is cut into its ports in key order: a port of
//!    `count` lamps takes that many, a zero port none, the last port with no
//!    count the rest; a count-less port before the last means the output
//!    drives no wire at all.

use super::v11_files::{read_json, read_text, refused, scalar};
use super::v11_patch::{PatchedRun, read_patch, resolve_patch};
use super::v11_project::{V11Fixture, V11Mapping, V11Output, V11Project};
use super::v11_project_landing::{
    DarkReason, FixtureLanding, FixtureLayout, LandedPiece, Landing, OutputLanding, ProjectLanding,
    SegmentLanding,
};
use super::v11_segments::{Map2dSegments, V11Segment, map2d_segments, path_points_segments};
use crate::json::JsonNode;
use crate::project_files::ProjectFiles;
use crate::upgrade_error::UpgradeError;

/// The project format whose landing this is.
const LANDING_PROJECT_FORMAT: u32 = 11;

/// Where every fixture lamp of the format-11 project in `files` lands
/// today, computed from the files alone.
pub fn landing(files: &ProjectFiles) -> Result<ProjectLanding, UpgradeError> {
    check_project_format(files)?;
    let project = V11Project::read(files)?;
    let names = project.output_names();

    // 1. Each fixture's lamps, segments and resolved runs.
    let mut fixtures: Vec<FixtureState> = Vec::with_capacity(project.fixtures.len());
    for fixture in &project.fixtures {
        fixtures.push(FixtureState::read(files, fixture, &names)?);
    }

    // 2–4. Each output's plan, onto its wires.
    let mut places: Vec<Vec<Vec<Place>>> = fixtures
        .iter()
        .map(|fixture| vec![Vec::new(); fixture.lamps as usize])
        .collect();
    let mut buffered: Vec<Vec<Option<usize>>> = fixtures
        .iter()
        .map(|fixture| vec![None; fixture.lamps as usize])
        .collect();
    let mut readers: Vec<Vec<usize>> = vec![Vec::new(); fixtures.len()];
    for (output_index, output) in project.outputs.iter().enumerate() {
        let providers = project.providers(output)?;
        for provider in &providers {
            readers[*provider].push(output_index);
        }
        let fragments = plan_fragments(&project, output, &providers, &fixtures);
        refuse_contested(output, &fragments)?;
        let wires = planned_wires(&output.ports);
        for fragment in &fragments {
            for (lamp, buffer_lamp) in fragment.lamps() {
                buffered[fragment.fixture][lamp as usize] = Some(output_index);
                let sample = buffer_lamp.saturating_mul(3);
                if let Some((port, start)) = wire_at(&wires, sample) {
                    places[fragment.fixture][lamp as usize].push(Place {
                        output: output_index,
                        port,
                        channel: sample - start,
                    });
                }
            }
        }
    }

    let mut landed = Vec::with_capacity(fixtures.len());
    for (index, (fixture, state)) in project.fixtures.iter().zip(&fixtures).enumerate() {
        let layout = match &state.segments {
            None => FixtureLayout::AlreadyFormat6,
            Some(segments) => {
                let lamps = Lamps {
                    project: &project,
                    channel: &fixture.channel,
                    state,
                    places: &places[index],
                    buffered: &buffered[index],
                    read: !readers[index].is_empty(),
                };
                FixtureLayout::Segments(
                    segments
                        .iter()
                        .map(|segment| lamps.segment_landing(&fixture.node, segment))
                        .collect::<Result<_, _>>()?,
                )
            }
        };
        landed.push(FixtureLanding {
            node: fixture.node.clone(),
            channel: fixture.channel.clone(),
            lamps: state.lamps,
            mapping_file: match &fixture.mapping {
                V11Mapping::Map2d { file } => Some(file.clone()),
                _ => None,
            },
            patch_file: fixture.patch_file.clone(),
            patch_unread: state.patch_unread.clone(),
            layout,
        });
    }
    Ok(ProjectLanding {
        fixtures: landed,
        outputs: project
            .outputs
            .iter()
            .map(|output| OutputLanding {
                node: output.node.clone(),
                name: output.name.clone(),
                channel: output.channel.clone(),
            })
            .collect(),
    })
}

/// A fixture as the engine holds it after its own produce: lamps, segments,
/// and its patch as the engine's routing reads it.
struct FixtureState {
    lamps: u32,
    /// `None`: an already-format-6 mapping.
    segments: Option<Vec<V11Segment>>,
    /// The resolved runs, after the dangling-name filter. `None` or empty:
    /// unpatched.
    runs: Option<Vec<PatchedRun>>,
    /// The runs the dangling-name filter dropped, for the dark reasons.
    dropped: Vec<PatchedRun>,
    manual: bool,
    /// Why the engine cannot read the fixture's patch, when it cannot.
    patch_unread: Option<String>,
}

impl FixtureState {
    fn read(
        files: &ProjectFiles,
        fixture: &V11Fixture,
        output_names: &[&str],
    ) -> Result<Self, UpgradeError> {
        let segments = match &fixture.mapping {
            V11Mapping::Unset => Some(Vec::new()),
            V11Mapping::Map2d { file } => {
                match map2d_segments(file, &read_json(files, file, &fixture.node)?)? {
                    Map2dSegments::Segments(segments) => Some(segments),
                    Map2dSegments::AlreadyFormat6 => {
                        if let Some(patch) = &fixture.patch_file {
                            return Err(refused(
                                patch,
                                &format!(
                                    "is a patch beside {file}, which is already format 6; the \
                                     new format holds its own patching"
                                ),
                            ));
                        }
                        None
                    }
                }
            }
            V11Mapping::PathPoints { paths } => Some(path_points_segments(&fixture.node, paths)?),
        };
        let lamps = match (&fixture.mapping, &segments) {
            (V11Mapping::PathPoints { paths }, _) => paths
                .iter()
                .filter(|(_, _, lamps)| *lamps > 0)
                .map(|(_, first, lamps)| first.saturating_add(*lamps))
                .max()
                .unwrap_or(0),
            (_, Some(segments)) => segments.iter().map(|segment| segment.lamps).sum(),
            (_, None) => 0,
        };
        let mut state = Self {
            lamps,
            segments,
            runs: None,
            dropped: Vec::new(),
            manual: false,
            patch_unread: None,
        };
        let (Some(patch_file), Some(segments)) = (&fixture.patch_file, &state.segments) else {
            return Ok(state);
        };
        // A patch file that is not there fails the engine's load (refused,
        // as `read_text` says); one that is there but unreadable auto-flows.
        let text = read_text(files, patch_file, &fixture.node)?;
        let doc = match JsonNode::parse(text)
            .map_err(|error| format!("it is not JSON: {}", error.detail))
            .and_then(|json| read_patch(&json))
        {
            Ok(doc) => doc,
            Err(reason) => {
                state.patch_unread = Some(reason);
                return Ok(state);
            }
        };
        state.manual = doc.manual;
        if doc.entries.is_empty() {
            return Ok(state);
        }
        // A document the kernel refuses falls back to "unpatched": auto-flow,
        // or nothing under manual.
        let Ok(runs) = resolve_patch(lamps, segments, &doc) else {
            return Ok(state);
        };
        // Dangling names are checked only once some output has a name.
        let (kept, dropped): (Vec<PatchedRun>, Vec<PatchedRun>) =
            runs.into_iter().partition(|run| match &run.output {
                Some(name) if !output_names.is_empty() => output_names.contains(&name.as_str()),
                _ => true,
            });
        state.runs = (!kept.is_empty()).then_some(kept);
        state.dropped = dropped;
        Ok(state)
    }
}

/// One planned fragment: `count` fixture lamps from `start`, at buffer lamp
/// `at`, reversed then rotated within the window (`OutputFragment`).
struct Fragment {
    fixture: usize,
    start: u32,
    count: u32,
    at: u32,
    reversed: bool,
    rotation: u32,
}

impl Fragment {
    /// `(fixture lamp, buffer lamp)` for each lamp: source lamp `j` sits at
    /// window slot `(j' + k) mod N` (`reverse_lamps`, then `rotate_lamps`).
    fn lamps(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        (0..self.count).map(move |j| {
            let flipped = if self.reversed { self.count - 1 - j } else { j };
            (
                self.start + j,
                self.at + (flipped + self.rotation) % self.count,
            )
        })
    }
}

/// The output's plan (`plan_fragments`), placement by placement in provider
/// order.
fn plan_fragments(
    project: &V11Project,
    output: &V11Output,
    providers: &[usize],
    fixtures: &[FixtureState],
) -> Vec<Fragment> {
    let is_default = project.is_default(output);
    // `Some(runs)`: place these; `None`: auto-flow the whole fixture.
    let placements: Vec<(usize, Option<Vec<&PatchedRun>>)> = providers
        .iter()
        .map(|&fixture| {
            let state = &fixtures[fixture];
            let placement = match &state.runs {
                None if is_default && !state.manual => None,
                None => Some(Vec::new()),
                Some(runs) => Some(
                    runs.iter()
                        .filter(|run| match &run.output {
                            None => is_default,
                            Some(name) => output.name.as_deref() == Some(name.as_str()),
                        })
                        .collect(),
                ),
            };
            (fixture, placement)
        })
        .collect();

    let mut cursor = placements
        .iter()
        .filter_map(|(_, runs)| runs.as_ref())
        .flatten()
        .map(|run| run.lamp_end())
        .max()
        .unwrap_or(0);
    let mut fragments = Vec::new();
    for (fixture, placement) in placements {
        match placement {
            Some(runs) => {
                for run in runs {
                    fragments.push(Fragment {
                        fixture,
                        start: run.start,
                        count: run.count,
                        at: run.lamp,
                        reversed: run.reversed,
                        rotation: run.offset,
                    });
                }
            }
            None => {
                let count = fixtures[fixture].lamps;
                fragments.push(Fragment {
                    fixture,
                    start: 0,
                    count,
                    at: cursor,
                    reversed: false,
                    rotation: 0,
                });
                cursor = cursor.saturating_add(count);
            }
        }
    }
    fragments
}

/// The output darkens a range two fragments claim. Nothing in the repo does
/// it, and an upgrade cannot say it, so it is refused by name.
fn refuse_contested(output: &V11Output, fragments: &[Fragment]) -> Result<(), UpgradeError> {
    let mut claimed: Vec<(u32, u32)> = fragments
        .iter()
        .filter(|fragment| fragment.count > 0)
        .map(|fragment| (fragment.at, fragment.at.saturating_add(fragment.count)))
        .collect();
    claimed.sort_unstable();
    for pair in claimed.windows(2) {
        if pair[1].0 < pair[0].1 {
            return Err(refused(
                &output.node,
                &format!(
                    "two runs claim its wire lamp {}; the engine darkens both, which the new \
                     format cannot say",
                    pair[1].0
                ),
            ));
        }
    }
    Ok(())
}

/// `(port key, first sample, samples)` per wire, exactly as the output
/// registers them; no wires at all when a port before the last omits its
/// count.
fn planned_wires(ports: &[(u32, Option<u32>)]) -> Vec<(u32, u32, Option<u32>)> {
    let mut wires = Vec::new();
    let mut start = 0u32;
    for (key, count) in ports {
        match count {
            Some(0) => {}
            Some(count) => {
                let len = count.saturating_mul(3);
                wires.push((*key, start, Some(len)));
                start = start.saturating_add(len);
            }
            None => wires.push((*key, start, None)),
        }
    }
    let offender = wires
        .iter()
        .take(wires.len().saturating_sub(1))
        .any(|(_, _, len)| len.is_none());
    if offender { Vec::new() } else { wires }
}

/// The wire carrying `sample`, as `(port key, its first sample)`.
fn wire_at(wires: &[(u32, u32, Option<u32>)], sample: u32) -> Option<(u32, u32)> {
    wires
        .iter()
        .find(|(_, start, len)| {
            sample >= *start && len.is_none_or(|len| sample < start.saturating_add(len))
        })
        .map(|(port, start, _)| (*port, *start))
}

/// One lamp on one wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Place {
    output: usize,
    port: u32,
    /// In samples, within the port.
    channel: u32,
}

/// A fixture's lamps once every output has placed them.
struct Lamps<'a> {
    project: &'a V11Project,
    /// The fixture's bus channel.
    channel: &'a str,
    state: &'a FixtureState,
    places: &'a [Vec<Place>],
    /// Which output's buffer holds each lamp, wire or not.
    buffered: &'a [Option<usize>],
    /// Whether any output reads the fixture.
    read: bool,
}

impl Lamps<'_> {
    fn segment_landing(
        &self,
        node: &str,
        segment: &V11Segment,
    ) -> Result<SegmentLanding, UpgradeError> {
        let first = segment.first_lamp as usize;
        let lamps = segment.lamps as usize;
        let places = &self.places[first..first + lamps];
        if let Some(lamp) = places.iter().position(|places| places.len() > 1) {
            return Err(refused(
                node,
                &format!(
                    "its lamp {} lands on {} wires; the frozen landing places a lamp once",
                    first + lamp,
                    places[lamp].len()
                ),
            ));
        }
        let placed: Vec<Option<Place>> = places
            .iter()
            .map(|places| places.first().copied())
            .collect();
        let landing = if placed.iter().all(Option::is_none) {
            Landing::Dark {
                why: self.dark_reason(segment.first_lamp),
            }
        } else if let Some(window) = whole_window(&placed) {
            window.landing(self.project)
        } else {
            Landing::Cut {
                why: cut_reason(&placed),
                pieces: pieces(self.project, segment.first_lamp, &placed),
            }
        };
        Ok(SegmentLanding {
            source: segment.source.clone(),
            first_lamp: segment.first_lamp,
            lamps: segment.lamps,
            landing,
        })
    }

    fn dark_reason(&self, lamp: u32) -> DarkReason {
        if !self.read {
            return DarkReason::NoOutputReads {
                channel: self.channel.to_string(),
            };
        }
        if let Some(output) = self.buffered[lamp as usize] {
            return DarkReason::OnNoPort {
                output: self.project.outputs[output].node.clone(),
            };
        }
        let run = self
            .state
            .runs
            .iter()
            .flatten()
            .chain(&self.state.dropped)
            .find(|run| lamp >= run.start && lamp < run.end());
        match run.and_then(|run| run.output.clone()) {
            Some(name) => DarkReason::RowNamesAnotherOutput { name },
            None => DarkReason::NotPatched,
        }
    }
}

/// The segment's lamps as one window on one port, if they are one.
struct Window {
    output: usize,
    port: u32,
    first_channel: u32,
    reversed: bool,
    rotation: u32,
}

impl Window {
    fn landing(&self, project: &V11Project) -> Landing {
        Landing::Landed {
            output: project.outputs[self.output].node.clone(),
            port: self.port,
            first_channel: self.first_channel,
            reversed: self.reversed,
            rotation: self.rotation,
        }
    }
}

/// Every lamp placed, on one port, filling `[low, low + n)` lamps of it as
/// one forward or reversed rotation of the segment.
fn whole_window(placed: &[Option<Place>]) -> Option<Window> {
    let places: Vec<Place> = placed.iter().copied().collect::<Option<_>>()?;
    let first = places[0];
    if places
        .iter()
        .any(|place| place.output != first.output || place.port != first.port)
    {
        return None;
    }
    let n = places.len() as u32;
    let low = places.iter().map(|place| place.channel).min()?;
    let slots: Vec<u32> = places
        .iter()
        .map(|place| (place.channel - low) / 3)
        .collect();
    if places.iter().any(|place| (place.channel - low) % 3 != 0) {
        return None;
    }
    let fits = |reversed: bool, rotation: u32| {
        slots.iter().enumerate().all(|(j, slot)| {
            let j = j as u32;
            let flipped = if reversed { n - 1 - j } else { j };
            *slot == (flipped + rotation) % n
        })
    };
    // A window of one or two lamps reads both ways (two lamps forward
    // rotated by one ARE the pair reversed); the description with the
    // smaller rotation wins, forward on a tie.
    let forward = slots[0] % n;
    let backward = (slots[0] + 1) % n;
    let candidates = [(false, forward), (true, backward)];
    let (reversed, rotation) = candidates
        .into_iter()
        .filter(|(reversed, rotation)| fits(*reversed, *rotation))
        .min_by_key(|(reversed, rotation)| (*rotation, *reversed))?;
    Some(Window {
        output: first.output,
        port: first.port,
        first_channel: low,
        reversed,
        rotation,
    })
}

fn cut_reason(placed: &[Option<Place>]) -> String {
    if placed.iter().any(Option::is_none) {
        return String::from("part of it lands nowhere");
    }
    let places: Vec<Place> = placed.iter().flatten().copied().collect();
    if places.iter().any(|place| place.output != places[0].output) {
        return String::from("its lamps land on two outputs");
    }
    if places.iter().any(|place| place.port != places[0].port) {
        return String::from("a port boundary falls inside it");
    }
    String::from("a row places only part of it")
}

/// The lamps that landed, as runs of consecutive lamps one lamp apart on one
/// port.
fn pieces(project: &V11Project, first_lamp: u32, placed: &[Option<Place>]) -> Vec<LandedPiece> {
    let mut pieces: Vec<(LandedPiece, usize)> = Vec::new();
    for (offset, place) in placed.iter().enumerate() {
        let Some(place) = place else { continue };
        let lamp = first_lamp + offset as u32;
        if let Some((piece, output)) = pieces.last_mut()
            && *output == place.output
            && piece.port == place.port
            && piece.first_lamp + piece.lamps == lamp
        {
            let step = i64::from(place.channel) - i64::from(piece.first_channel);
            if piece.lamps == 1 && (step == 3 || step == -3) {
                piece.step = step as i32;
                piece.lamps += 1;
                continue;
            }
            let next =
                i64::from(piece.first_channel) + i64::from(piece.step) * i64::from(piece.lamps);
            if piece.lamps > 1 && next == i64::from(place.channel) {
                piece.lamps += 1;
                continue;
            }
        }
        pieces.push((
            LandedPiece {
                first_lamp: lamp,
                lamps: 1,
                output: project.outputs[place.output].node.clone(),
                port: place.port,
                first_channel: place.channel,
                step: 3,
            },
            place.output,
        ));
    }
    pieces.into_iter().map(|(piece, _)| piece).collect()
}

fn check_project_format(files: &ProjectFiles) -> Result<(), UpgradeError> {
    let manifest = read_json(files, "/project.json", "project.json")?;
    let format = manifest.get("format").and_then(scalar::<u32>);
    if format != Some(LANDING_PROJECT_FORMAT) {
        return Err(refused(
            "project.json",
            &format!(
                "the frozen landing reads format-{LANDING_PROJECT_FORMAT} projects; this one \
                 says {format:?}"
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows place first, and a fixture's unclaimed lamps flow on after its
    /// highest anchored end on the default output.
    #[test]
    fn rows_place_first_and_the_rest_flows_on_after_them() {
        let landed = landing(&project(
            &[
                fixture("f", "f", Some("f.patch.json")),
                output("o", None, &[(0, None)]),
            ],
            &[
                ("f.map2d.json", &paths(&[("a", 2), ("b", 3)])),
                ("f.patch.json", r#"{"format":2,"entries":[["/b",-1,10]]}"#),
            ],
        ))
        .unwrap();
        assert_eq!(
            landings(&landed, "/f.json"),
            vec![
                landed_at("/o.json", 0, 13 * 3),
                landed_at("/o.json", 0, 10 * 3)
            ]
        );
    }

    /// A row naming another output neither takes nor moves the default
    /// output's cursor: the rest flows on from 0.
    #[test]
    fn a_named_outputs_row_does_not_move_the_default_outputs_cursor() {
        let landed = landing(&project(
            &[
                fixture("f", "f", Some("f.patch.json")),
                output("a", Some("A"), &[(0, None)]),
                output("b", Some("B"), &[(0, None)]),
            ],
            &[
                ("f.map2d.json", &paths(&[("x", 2), ("y", 3)])),
                (
                    "f.patch.json",
                    r#"{"format":2,"outputs":["B"],"entries":[["/x",0,40]]}"#,
                ),
            ],
        ))
        .unwrap();
        assert_eq!(
            landings(&landed, "/f.json"),
            vec![landed_at("/b.json", 0, 40 * 3), landed_at("/a.json", 0, 0)]
        );
    }

    /// Under `flow: manual` the lamps no row places are dark.
    #[test]
    fn manual_flow_leaves_the_rest_dark() {
        let landed = landing(&project(
            &[
                fixture("f", "f", Some("f.patch.json")),
                output("o", None, &[(0, None)]),
            ],
            &[
                ("f.map2d.json", &paths(&[("a", 2), ("b", 3)])),
                (
                    "f.patch.json",
                    r#"{"format":3,"flow":"manual","entries":[["/b",-1,0]]}"#,
                ),
            ],
        ))
        .unwrap();
        assert_eq!(
            landings(&landed, "/f.json"),
            vec![
                Landing::Dark {
                    why: DarkReason::NotPatched
                },
                landed_at("/o.json", 0, 0),
            ]
        );
    }

    /// A fixture with no rows flows on after every run on its output — the
    /// other fixtures' anchors included.
    #[test]
    fn an_unpatched_fixture_flows_after_every_run_on_its_output() {
        let landed = landing(&project(
            &[
                fixture("f", "f", Some("f.patch.json")),
                fixture("g", "g", None),
                output("o", None, &[(0, None)]),
            ],
            &[
                ("f.map2d.json", &paths(&[("a", 4)])),
                ("f.patch.json", r#"{"format":2,"entries":[["/a",-1,20]]}"#),
                ("g.map2d.json", &paths(&[("a", 3)])),
            ],
        ))
        .unwrap();
        assert_eq!(
            landings(&landed, "/g.json"),
            vec![landed_at("/o.json", 0, 24 * 3)]
        );
    }

    /// Two unpatched fixtures on one output flow one after the other, in
    /// provider order (load order: keys `a_first` before `b_second`).
    #[test]
    fn two_fixtures_flow_in_provider_order() {
        let landed = landing(&project(
            &[
                fixture("b_second", "b", None),
                fixture("a_first", "a", None),
                output("o", None, &[(0, None)]),
            ],
            &[
                ("a.map2d.json", &paths(&[("x", 5)])),
                ("b.map2d.json", &paths(&[("x", 3)])),
            ],
        ))
        .unwrap();
        assert_eq!(
            landings(&landed, "/a.json"),
            vec![landed_at("/o.json", 0, 0)]
        );
        assert_eq!(
            landings(&landed, "/b.json"),
            vec![landed_at("/o.json", 0, 5 * 3)]
        );
    }

    /// A rotated, reversed row keeps the segment one window: reversal first,
    /// then the rotation.
    #[test]
    fn a_rotated_reversed_row_lands_as_one_window() {
        let landed = landing(&project(
            &[
                fixture("f", "f", Some("f.patch.json")),
                output("o", None, &[(0, None)]),
            ],
            &[
                ("f.map2d.json", &paths(&[("a", 5)])),
                (
                    "f.patch.json",
                    r#"{"format":2,"entries":[["/a",-1,10,"r",2]]}"#,
                ),
            ],
        ))
        .unwrap();
        assert_eq!(
            landings(&landed, "/f.json"),
            vec![Landing::Landed {
                output: String::from("/o.json"),
                port: 0,
                first_channel: 30,
                reversed: true,
                rotation: 2,
            }]
        );
    }

    /// A range covering two whole segments lands each as its own window.
    #[test]
    fn a_range_over_two_whole_segments_lands_each_whole() {
        let landed = landing(&project(
            &[fixture("f", "f", Some("f.patch.json")), output("o", None, &[(0, None)])],
            &[
                ("f.map2d.json", &paths(&[("a", 2), ("b", 3)])),
                (
                    "f.patch.json",
                    r#"{"format":1,"entries":[{"range":{"start":0,"count":5},"at":{"channel":7},"reversed":true}]}"#,
                ),
            ],
        ))
        .unwrap();
        let reversed_at = |first_channel: u32| Landing::Landed {
            output: String::from("/o.json"),
            port: 0,
            first_channel,
            reversed: true,
            rotation: 0,
        };
        assert_eq!(
            landings(&landed, "/f.json"),
            vec![reversed_at(10 * 3), reversed_at(7 * 3)]
        );
    }

    /// A sub-span placing part of a segment away from the rest cuts it
    /// (lamps 2–3 at wire lamp 20, the rest reflowing after them: 0–1 at
    /// 22, 4–5 at 24).
    #[test]
    fn a_sub_span_cuts_its_segment() {
        let landed = landing(&project(
            &[
                fixture("f", "f", Some("f.patch.json")),
                output("o", None, &[(0, None)]),
            ],
            &[
                ("f.map2d.json", &paths(&[("a", 6)])),
                (
                    "f.patch.json",
                    r#"{"format":2,"entries":[[["/a",2,2],-1,20]]}"#,
                ),
            ],
        ))
        .unwrap();
        let [Landing::Cut { why, pieces }] = &landings(&landed, "/f.json")[..] else {
            panic!("one cut segment");
        };
        assert_eq!(why, "a row places only part of it");
        let runs: Vec<(u32, u32, u32)> = pieces
            .iter()
            .map(|piece| (piece.first_lamp, piece.lamps, piece.first_channel / 3))
            .collect();
        assert_eq!(runs, vec![(0, 2, 22), (2, 2, 20), (4, 2, 24)]);
    }

    /// A port boundary inside a segment cuts it.
    #[test]
    fn a_port_boundary_inside_a_segment_cuts_it() {
        let landed = landing(&project(
            &[
                fixture("f", "f", None),
                output("o", None, &[(0, Some(4)), (1, None)]),
            ],
            &[("f.map2d.json", &paths(&[("a", 6)]))],
        ))
        .unwrap();
        let [Landing::Cut { why, pieces }] = &landings(&landed, "/f.json")[..] else {
            panic!("one cut segment");
        };
        assert_eq!(why, "a port boundary falls inside it");
        assert_eq!(
            (pieces[0].port, pieces[1].port, pieces[1].first_channel),
            (0, 1, 0)
        );
    }

    /// A fixture whose channel no output reads is dark, and says so.
    #[test]
    fn a_fixture_no_output_reads_is_dark_with_the_reason() {
        let unread = (
            "f",
            String::from("f.json"),
            String::from(
                r#"{"kind":"Fixture","bindings":{"output":{"target":"bus:control.out/ch9"}},"mapping":{"kind":"Map2d","source":"f.map2d.json"}}"#,
            ),
        );
        let landed = landing(&project(
            &[unread, output("o", None, &[(0, None)])],
            &[("f.map2d.json", &paths(&[("a", 3)]))],
        ))
        .unwrap();
        assert_eq!(
            landings(&landed, "/f.json"),
            vec![Landing::Dark {
                why: DarkReason::NoOutputReads {
                    channel: String::from("control.out/ch9")
                }
            }]
        );
    }

    /// Of two outputs on one channel the first in load order — keys
    /// compared as strings — is the default.
    #[test]
    fn of_two_outputs_on_one_channel_the_first_in_load_order_is_the_default() {
        let landed = landing(&project(
            &[
                fixture("f", "f", None),
                output("out_2", None, &[(0, None)]),
                output("out_10", None, &[(0, None)]),
            ],
            &[("f.map2d.json", &paths(&[("a", 3)]))],
        ))
        .unwrap();
        assert_eq!(
            landings(&landed, "/f.json"),
            vec![landed_at("/out_10.json", 0, 0)]
        );
    }

    /// A frozen surprise: a row naming an output no output is named is
    /// dropped — and when it was the fixture's only run, the fixture counts
    /// as unpatched and auto-flows on the default output.
    #[test]
    fn a_fixture_whose_only_row_names_a_missing_output_auto_flows() {
        let landed = landing(&project(
            &[
                fixture("f", "f", Some("f.patch.json")),
                output("o", Some("real"), &[(0, None)]),
            ],
            &[
                ("f.map2d.json", &paths(&[("a", 4)])),
                (
                    "f.patch.json",
                    r#"{"format":2,"outputs":["ghost"],"entries":[["/a",0,9]]}"#,
                ),
            ],
        ))
        .unwrap();
        assert_eq!(
            landings(&landed, "/f.json"),
            vec![landed_at("/o.json", 0, 0)]
        );
    }

    /// A frozen surprise: a row naming a missing output beside other runs
    /// is dropped and its lamps go dark — the reflow already counted them
    /// claimed.
    #[test]
    fn a_row_naming_a_missing_output_beside_others_goes_dark() {
        let landed = landing(&project(
            &[
                fixture("f", "f", Some("f.patch.json")),
                output("o", Some("real"), &[(0, None)]),
            ],
            &[
                ("f.map2d.json", &paths(&[("a", 2), ("b", 2)])),
                (
                    "f.patch.json",
                    r#"{"format":2,"outputs":["ghost"],"entries":[["/a",0,9]]}"#,
                ),
            ],
        ))
        .unwrap();
        assert_eq!(
            landings(&landed, "/f.json"),
            vec![
                Landing::Dark {
                    why: DarkReason::RowNamesAnotherOutput {
                        name: String::from("ghost")
                    }
                },
                landed_at("/o.json", 0, 0),
            ]
        );
    }

    /// A frozen surprise: a manual patch the kernel refuses (two rows claim
    /// one lamp) places nothing at all.
    #[test]
    fn a_manual_patch_the_kernel_refuses_places_nothing() {
        let landed = landing(&project(
            &[
                fixture("f", "f", Some("f.patch.json")),
                output("o", None, &[(0, None)]),
            ],
            &[
                ("f.map2d.json", &paths(&[("a", 6)])),
                (
                    "f.patch.json",
                    r#"{"format":3,"flow":"manual","entries":[[[0,4],-1,0],[[2,2],-1,10]]}"#,
                ),
            ],
        ))
        .unwrap();
        assert_eq!(
            landings(&landed, "/f.json"),
            vec![Landing::Dark {
                why: DarkReason::NotPatched
            }]
        );
    }

    /// An auto patch the kernel refuses falls back to auto-flow.
    #[test]
    fn an_auto_patch_the_kernel_refuses_auto_flows() {
        let landed = landing(&project(
            &[
                fixture("f", "f", Some("f.patch.json")),
                output("o", None, &[(0, None)]),
            ],
            &[
                ("f.map2d.json", &paths(&[("a", 6)])),
                ("f.patch.json", r#"{"format":2,"entries":[["/nope",-1,9]]}"#),
            ],
        ))
        .unwrap();
        assert_eq!(
            landings(&landed, "/f.json"),
            vec![landed_at("/o.json", 0, 0)]
        );
    }

    /// A patch the engine cannot read at all (not JSON, or a format it does
    /// not know) is attached with no placement: the fixture auto-flows, and
    /// the landing says why (DD8).
    #[test]
    fn an_unreadable_patch_auto_flows_and_says_why() {
        for text in [r#"{"format":9,"entries":[]}"#, "not json"] {
            let landed = landing(&project(
                &[
                    fixture("f", "f", Some("f.patch.json")),
                    output("o", None, &[(0, None)]),
                ],
                &[
                    ("f.map2d.json", &paths(&[("a", 6)])),
                    ("f.patch.json", text),
                ],
            ))
            .unwrap();
            assert_eq!(
                landings(&landed, "/f.json"),
                vec![landed_at("/o.json", 0, 0)]
            );
            assert!(landed.fixtures[0].patch_unread.is_some(), "{text}");
        }
    }

    /// A patch file the fixture names but the project lacks fails the
    /// engine's load, so the landing refuses it.
    #[test]
    fn a_missing_patch_file_is_refused() {
        let files = project(
            &[
                fixture("f", "f", Some("f.patch.json")),
                output("o", None, &[(0, None)]),
            ],
            &[("f.map2d.json", &paths(&[("a", 6)]))],
        );
        assert!(landing(&files).is_err());
    }

    /// Lamps past the last counted port land on no wire.
    #[test]
    fn lamps_past_the_last_counted_port_are_dark() {
        let landed = landing(&project(
            &[
                fixture("f", "f", None),
                fixture("g", "g", None),
                output("o", None, &[(0, Some(3))]),
            ],
            &[
                ("f.map2d.json", &paths(&[("a", 3)])),
                ("g.map2d.json", &paths(&[("a", 2)])),
            ],
        ))
        .unwrap();
        assert_eq!(
            landings(&landed, "/g.json"),
            vec![Landing::Dark {
                why: DarkReason::OnNoPort {
                    output: String::from("/o.json")
                }
            }]
        );
    }

    /// A port before the last with no count: the output drives no wire.
    #[test]
    fn a_countless_port_before_the_last_drives_no_wire() {
        let landed = landing(&project(
            &[
                fixture("f", "f", None),
                output("o", None, &[(0, None), (1, Some(4))]),
            ],
            &[("f.map2d.json", &paths(&[("a", 3)]))],
        ))
        .unwrap();
        assert_eq!(
            landings(&landed, "/f.json"),
            vec![Landing::Dark {
                why: DarkReason::OnNoPort {
                    output: String::from("/o.json")
                }
            }]
        );
    }

    /// A map2d already at format 6 is passed through, unread.
    #[test]
    fn a_format_6_mapping_is_passed_through() {
        let landed = landing(&project(
            &[fixture("f", "f", None), output("o", None, &[(0, None)])],
            &[("f.map2d.json", r#"{"format":6,"parts":{}}"#)],
        ))
        .unwrap();
        assert_eq!(landed.fixtures[0].layout, FixtureLayout::AlreadyFormat6);
    }

    /// A format-6 map2d with a patch file beside it is refused.
    #[test]
    fn a_format_6_mapping_with_a_patch_beside_it_is_refused() {
        let error = landing(&project(
            &[
                fixture("f", "f", Some("f.patch.json")),
                output("o", None, &[(0, None)]),
            ],
            &[
                ("f.map2d.json", r#"{"format":6,"parts":{}}"#),
                ("f.patch.json", r#"{"format":2,"entries":[]}"#),
            ],
        ))
        .unwrap_err();
        assert!(error.to_string().contains("already format 6"), "{error}");
    }

    // --- helpers -----------------------------------------------------------

    type Node = (&'static str, String, String);

    fn landings(landed: &ProjectLanding, node: &str) -> Vec<Landing> {
        let fixture = landed
            .fixtures
            .iter()
            .find(|fixture| fixture.node == node)
            .expect("fixture");
        let FixtureLayout::Segments(segments) = &fixture.layout else {
            panic!("segments");
        };
        segments
            .iter()
            .map(|segment| segment.landing.clone())
            .collect()
    }

    fn landed_at(output: &str, port: u32, first_channel: u32) -> Landing {
        Landing::Landed {
            output: String::from(output),
            port,
            first_channel,
            reversed: false,
            rotation: 0,
        }
    }

    /// A fixture node keyed `key`, its map2d `<stem>.map2d.json`.
    fn fixture(key: &'static str, stem: &str, patch: Option<&str>) -> Node {
        let patch = patch
            .map(|file| format!(r#","patch":{{"kind":"File","source":"{file}"}}"#))
            .unwrap_or_default();
        (
            key,
            format!("{stem}.json"),
            format!(
                r#"{{"kind":"Fixture","mapping":{{"kind":"Map2d","source":"{stem}.map2d.json"}}{patch}}}"#
            ),
        )
    }

    /// An output node keyed `key` (file `<key>.json`), its ports `(key,
    /// count)`.
    fn output(key: &'static str, name: Option<&str>, ports: &[(u32, Option<u32>)]) -> Node {
        let name = name
            .map(|name| format!(r#""name":"{name}","#))
            .unwrap_or_default();
        let ports: Vec<String> = ports
            .iter()
            .map(|(port, count)| match count {
                Some(count) => {
                    format!(r#""{port}":{{"endpoint":"ws281x:local:D{port}","count":{count}}}"#)
                }
                None => format!(r#""{port}":{{"endpoint":"ws281x:local:D{port}"}}"#),
            })
            .collect();
        (
            key,
            format!("{key}.json"),
            format!(
                r#"{{"kind":"Output",{name}"ports":{{{}}}}}"#,
                ports.join(",")
            ),
        )
    }

    /// A format-3 map2d of plain paths `(id, lamps)`.
    fn paths(objects: &[(&str, u32)]) -> String {
        let objects: Vec<String> = objects
            .iter()
            .map(|(id, count)| {
                format!(
                    r#"{{"id":"{id}","shape":{{"path":{{"points":[[0,0],[10,0]],"count":{count}}}}}}}"#
                )
            })
            .collect();
        format!(r#"{{"format":3,"objects":[{}]}}"#, objects.join(","))
    }

    /// A format-11 project: a root module holding `nodes`, plus `extra`
    /// files.
    fn project(nodes: &[Node], extra: &[(&str, &str)]) -> ProjectFiles {
        let refs: Vec<String> = nodes
            .iter()
            .map(|(key, file, _)| format!(r#""{key}":{{"ref":"./{file}"}}"#))
            .collect();
        let mut files: Vec<(String, Vec<u8>)> = vec![
            (String::from("project.json"), br#"{"format":11}"#.to_vec()),
            (
                String::from("module.json"),
                format!(r#"{{"kind":"Module","nodes":{{{}}}}}"#, refs.join(",")).into_bytes(),
            ),
        ];
        for (_, file, json) in nodes {
            files.push((file.clone(), json.clone().into_bytes()));
        }
        for (path, text) in extra {
            files.push((path.to_string(), text.as_bytes().to_vec()));
        }
        files.into_iter().collect()
    }
}
