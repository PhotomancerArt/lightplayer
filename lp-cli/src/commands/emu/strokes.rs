//! `lp-cli emu run --host-link --strokes <spec>`: draw on the emulated board.
//!
//! After `--upload`, the host writes touch sets to the project's `touches`
//! channel the way Studio does while someone drags on a board's picture: a
//! momentary panel write (`ttl_ms`) of a `u32`-keyed map of
//! `lp::touch::Touch`, at most one in flight, at the script's pace in
//! EMULATED time. A sample whose moment has passed while the previous write
//! was in flight is dropped in favour of the newest, as Studio collapses
//! them. A stroke ends with one empty set, then silence, so the board
//! releases on its own when the ttl runs out.
//!
//! The spec is a built-in (`line`, `circle`, `two`) or a JSON file holding a
//! [`StrokeScript`]. Built-ins are 30 samples a second in pattern space
//! (the lamps' box, long side −1…1, y up) with a 400 ms ttl, the numbers
//! Studio sends.

use std::path::Path;

use anyhow::{Context, Result, bail};
use lpa_client::LpClient;
use lpc_model::{PanelWriteValue, TOUCHES_CHANNEL, ToLpValue, Touch};
use lpc_wire::{WirePanelCommandResponse, WirePanelWriteRequest};
use serde::{Deserialize, Serialize};

use super::link_host::{EmuLinkHost, EmuUsbBoard};

/// Request ids for the stroke's writes: far from the client's own and from
/// `--request`'s.
const STROKE_REQUEST_ID_BASE: u64 = 2_000_000;

/// Studio's send interval while drawing (≤ 30 Hz).
const SAMPLE_MS: u64 = 33;
/// Studio's momentary ttl for a touch set.
const TTL_MS: u32 = 400;

/// A scripted drawing: touch sets at moments after the stroke starts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrokeScript {
    /// The momentary ttl every write carries.
    pub ttl_ms: u32,
    /// In time order. An empty `touches` is a release.
    pub samples: Vec<StrokeSample>,
}

/// One write: the whole touch set at `at_ms` after the stroke starts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrokeSample {
    pub at_ms: u64,
    /// `[id, x, y]` per finger, pattern space; ids are ≥ 1.
    pub touches: Vec<[f32; 3]>,
}

/// What drawing did, for the run's report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StrokeCounts {
    pub sent: u32,
    pub accepted: u32,
    pub rejected: u32,
    /// Samples dropped because a newer one was already due.
    pub collapsed: u32,
}

impl StrokeScript {
    /// A built-in by name, `duration_ms` long, or a JSON file.
    pub fn from_spec(spec: &str, duration_ms: u64) -> Result<Self> {
        let path = Path::new(spec);
        if path.extension().is_some_and(|ext| ext == "json") {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading the stroke script {}", path.display()))?;
            let script: Self = serde_json::from_str(&text)
                .with_context(|| format!("{} is not a stroke script", path.display()))?;
            script.check()?;
            return Ok(script);
        }
        let script = match spec {
            "line" => Self::generate(duration_ms, |t| vec![line_finger(1, t)]),
            "circle" => Self::generate(duration_ms, |t| vec![circle_finger(1, t, 0.5, 0.0)]),
            "two" => Self::generate(duration_ms, |t| {
                vec![circle_finger(1, t, 0.5, 0.0), circle_finger(2, t, 0.3, 0.5)]
            }),
            other => bail!("--strokes `{other}`: expected line, circle, two or a .json script"),
        };
        script.check()?;
        Ok(script)
    }

    fn generate(duration_ms: u64, fingers: impl Fn(f32) -> Vec<[f32; 3]>) -> Self {
        let mut samples: Vec<StrokeSample> = (0..=duration_ms / SAMPLE_MS)
            .map(|i| {
                let at_ms = i * SAMPLE_MS;
                StrokeSample {
                    at_ms,
                    touches: fingers(at_ms as f32 / 1000.0),
                }
            })
            .collect();
        let end = samples.last().map_or(0, |s| s.at_ms) + SAMPLE_MS;
        samples.push(StrokeSample {
            at_ms: end,
            touches: Vec::new(),
        });
        Self {
            ttl_ms: TTL_MS,
            samples,
        }
    }

    fn check(&self) -> Result<()> {
        if self.ttl_ms == 0 {
            bail!("a stroke script's ttl_ms must be above 0");
        }
        if self.samples.windows(2).any(|w| w[1].at_ms < w[0].at_ms) {
            bail!("a stroke script's samples must be in time order");
        }
        if self
            .samples
            .iter()
            .flat_map(|s| &s.touches)
            .any(|[id, _, _]| *id < 1.0 || id.fract() != 0.0)
        {
            bail!("a stroke script's finger ids must be whole numbers ≥ 1");
        }
        Ok(())
    }

    /// The emulated span from the first sample to the last.
    pub fn span_ms(&self) -> u64 {
        self.samples.last().map_or(0, |s| s.at_ms)
    }
}

/// A finger sweeping left and right across the middle, 2 s each way.
fn line_finger(id: u32, t: f32) -> [f32; 3] {
    let phase = (t / 4.0).fract();
    let x = if phase < 0.5 {
        -0.8 + 3.2 * phase
    } else {
        0.8 - 3.2 * (phase - 0.5)
    };
    [id as f32, x, 0.0]
}

/// A finger circling the centre once every 3 s, starting `phase` turns in.
fn circle_finger(id: u32, t: f32, radius: f32, phase: f32) -> [f32; 3] {
    let angle = core::f32::consts::TAU * (t / 3.0 + phase);
    [id as f32, radius * angle.cos(), radius * angle.sin()]
}

/// Draw `script` on the board, starting now. Stops early at `deadline_us`.
pub async fn draw<B: EmuUsbBoard>(
    host: &mut EmuLinkHost<B>,
    script: &StrokeScript,
    deadline_us: u64,
) -> Result<StrokeCounts> {
    let mut counts = StrokeCounts::default();
    host.set_queue_messages(true);
    let mut client = LpClient::new(&mut *host).with_request_ids_from(STROKE_REQUEST_ID_BASE);
    let loaded = client
        .project_list_loaded()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .value;
    let handle = loaded
        .first()
        .context("--strokes: no project is loaded")?
        .handle;
    let read: lpc_wire::ProjectReadRequest = serde_json::from_value(serde_json::json!({
        "since": null,
        "probes": [{"binding_graph": {"structure": "always", "include_values": false}}]
    }))
    .expect("a binding-graph read");
    let events = client
        .project_read(handle, read)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .value;
    let graph = serde_json::to_value(&events).expect("events as JSON");
    let scope = find_channel_scope(&graph, TOUCHES_CHANNEL)
        .context("--strokes: the project has no `touches` channel to draw on")?;
    let scope = serde_json::from_value(scope).context("the `touches` scope")?;
    drop(client);

    let start_us = host.board.micros();
    let mut samples = script.samples.iter().peekable();
    while let Some(sample) = samples.next() {
        // Collapse: a newer sample is already due, so this one is stale.
        let now_us = host.board.micros();
        if samples
            .peek()
            .is_some_and(|next| start_us + next.at_ms * 1000 <= now_us)
        {
            counts.collapsed += 1;
            continue;
        }
        let due_us = start_us + sample.at_ms * 1000;
        if due_us > deadline_us {
            break;
        }
        if now_us < due_us {
            host.set_queue_messages(false);
            host.run_until(due_us, None)?;
            host.set_queue_messages(true);
        }
        let set = sample
            .touches
            .iter()
            .map(|[id, x, y]| {
                let id = *id as u32;
                (id, Touch::new(id, [*x, *y]).to_lp_value())
            })
            .collect();
        let request = WirePanelWriteRequest {
            scope,
            channel: TOUCHES_CHANNEL.to_string(),
            value: PanelWriteValue::Map(set),
            ttl_ms: Some(script.ttl_ms),
        };
        let mut client = LpClient::new(&mut *host)
            .with_request_ids_from(STROKE_REQUEST_ID_BASE + 1 + u64::from(counts.sent));
        let response = client
            .project_panel_write(handle, request)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .value;
        counts.sent += 1;
        match response {
            WirePanelCommandResponse::Accepted { .. } => counts.accepted += 1,
            WirePanelCommandResponse::Rejected { reason } => {
                counts.rejected += 1;
                if counts.rejected == 1 {
                    eprintln!("emu: strokes — a touch write was refused: {reason}");
                }
            }
        }
    }
    host.set_queue_messages(false);
    Ok(counts)
}

/// The `scope` of the channel named `name`, anywhere in a read's JSON.
fn find_channel_scope(v: &serde_json::Value, name: &str) -> Option<serde_json::Value> {
    match v {
        serde_json::Value::Object(map) => {
            if map.get("name").and_then(|n| n.as_str()) == Some(name)
                && let Some(scope) = map.get("scope")
                && !scope.is_null()
            {
                return Some(scope.clone());
            }
            map.values().find_map(|v| find_channel_scope(v, name))
        }
        serde_json::Value::Array(items) => items.iter().find_map(|v| find_channel_scope(v, name)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_built_in_stroke_is_thirty_samples_a_second_then_a_release() {
        let script = StrokeScript::from_spec("circle", 1_000).expect("circle");
        assert_eq!(script.ttl_ms, 400);
        let (release, strokes) = script.samples.split_last().expect("samples");
        assert!(release.touches.is_empty(), "the stroke ends with a release");
        assert_eq!(strokes.len(), 31, "0..=1000 ms at 33 ms");
        assert!(
            strokes
                .windows(2)
                .all(|w| w[1].at_ms - w[0].at_ms == SAMPLE_MS)
        );
        assert!(strokes.iter().all(|s| s.touches.len() == 1));
    }

    #[test]
    fn two_fingers_have_two_ids() {
        let script = StrokeScript::from_spec("two", 100).expect("two");
        let ids: Vec<f32> = script.samples[0].touches.iter().map(|t| t[0]).collect();
        assert_eq!(ids, [1.0, 2.0]);
    }

    #[test]
    fn a_line_stays_inside_the_lamps() {
        let script = StrokeScript::from_spec("line", 8_000).expect("line");
        assert!(
            script
                .samples
                .iter()
                .flat_map(|s| &s.touches)
                .all(|[_, x, y]| x.abs() <= 0.8 + 1e-4 && *y == 0.0)
        );
    }

    #[test]
    fn a_script_file_round_trips_and_is_checked() {
        let script = StrokeScript {
            ttl_ms: 400,
            samples: vec![
                StrokeSample {
                    at_ms: 0,
                    touches: vec![[1.0, 0.0, 0.5]],
                },
                StrokeSample {
                    at_ms: 33,
                    touches: vec![],
                },
            ],
        };
        let json = serde_json::to_string(&script).unwrap();
        assert_eq!(serde_json::from_str::<StrokeScript>(&json).unwrap(), script);

        let bad = StrokeScript {
            ttl_ms: 400,
            samples: vec![StrokeSample {
                at_ms: 0,
                touches: vec![[0.0, 0.0, 0.0]],
            }],
        };
        assert!(bad.check().is_err(), "id 0 marks an empty slot");
    }

    #[test]
    fn an_unknown_built_in_is_an_error() {
        assert!(StrokeScript::from_spec("zigzag", 1_000).is_err());
    }
}
