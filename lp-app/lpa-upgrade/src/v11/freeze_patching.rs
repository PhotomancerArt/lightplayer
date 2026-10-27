//! Today's landing of one fixture, frozen as format 6's patching: the
//! fixture's start and an override on every segment that does not simply
//! follow on (lean Q7: simple and correct, not minimal).
//!
//! The segments are walked in order with a **predicted cursor** that moves
//! as the patcher's does (`lpc_mapping::patching::patcher`, M1 P2), on ports
//! of `count × 3` channels (a port with no count takes the rest when it is
//! the last, and has no room before it):
//!
//! - **The start** is the first landed segment's address: output Set (by
//!   name), port Set (its key), channel Set in lamps. A fixture none of
//!   whose segments lands gets **no start** and no overrides (lean Q8): the
//!   patcher reports "no start" and lights nothing, as today.
//! - **A dark segment** (no row places it under manual, a row names an
//!   output that is not reading it, past the last port, no output reads the
//!   fixture) gets `off`: no span, and the cursor does not move past it.
//! - **A landed segment** whose address is where the cursor would lay it —
//!   at the cursor, or on the next port with room from its channel 0 when
//!   it does not fit what is left (the wrap rule counts as following on) —
//!   gets no override for its address. Any other gets port Set and channel
//!   Set (in lamps), and output Set only when its output is not the
//!   cursor's.
//! - Reversed adds `direction: reverse`; a rotation `k` adds `offset: k`
//!   (today's `(j' + k) mod N` is the patcher's).
//! - The cursor then sits at the end of the segment's true address.
//!
//! Every Set the freeze writes is followed by a segment that fits where it
//! was set (today landed it there whole), so the patcher's "first segment
//! after a Set does not wrap" rule never changes a landing, and no segment
//! ever runs past the last port (one that would is either dark, so off, or
//! landed, so set where it lands).

use super::v11_project_landing::{Landing, SegmentLanding};

/// Channels per lamp: every format-11 fixture is RGB (`color_order` moves
/// the bytes, never their count; DD12 adds no lamp fields).
const CHANNELS_PER_LAMP: u32 = 3;

/// A fixture's frozen patching. Outputs are named by their definition file
/// here; the step names them for the document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct FrozenPatching {
    pub(crate) start: Option<FrozenAddress>,
    /// By segment node id, in segment order (the document sorts them).
    pub(crate) overrides: Vec<(String, FrozenOverride)>,
}

/// Where a segment's run starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrozenAddress {
    /// The output's definition file.
    pub(crate) output: String,
    pub(crate) port: u32,
    /// In lamps.
    pub(crate) lamp: u32,
}

/// One segment's override: each `None` takes what the segment would get.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct FrozenOverride {
    pub(crate) output: Option<String>,
    pub(crate) port: Option<u32>,
    /// Channel Set, in lamps.
    pub(crate) lamp: Option<u32>,
    pub(crate) off: bool,
    pub(crate) reverse: bool,
    /// In lamps; 0 is none.
    pub(crate) offset: u32,
}

/// One output's ports as the patcher is given them: `(key, capacity in
/// lamps)`, in key order.
pub(crate) type OutputPorts<'a> = (&'a str, &'a [(u32, Option<u32>)]);

/// Freeze one fixture's landing. `segments` pairs each segment's node id
/// with today's landing, in the fixture's lamp order. `Err` names a segment
/// that is not one window on one port (the step refuses those first) or an
/// address the patcher cannot be given.
pub(crate) fn freeze_patching(
    segments: &[(String, &SegmentLanding)],
    outputs: &[OutputPorts<'_>],
) -> Result<FrozenPatching, String> {
    let mut frozen = FrozenPatching::default();
    if !segments
        .iter()
        .any(|(_, segment)| matches!(segment.landing, Landing::Landed { .. }))
    {
        return Ok(frozen);
    }
    let mut cursor: Option<Cursor<'_>> = None;
    for (id, segment) in segments {
        let (output, port, first_channel, reversed, rotation) = match &segment.landing {
            Landing::Dark { .. } => {
                frozen.overrides.push((
                    id.clone(),
                    FrozenOverride {
                        off: true,
                        ..FrozenOverride::default()
                    },
                ));
                continue;
            }
            Landing::Cut { why, .. } => return Err(format!("{id}: {why}")),
            Landing::Landed {
                output,
                port,
                first_channel,
                reversed,
                rotation,
            } => (output, *port, *first_channel, *reversed, *rotation),
        };
        if first_channel % CHANNELS_PER_LAMP != 0 {
            return Err(format!(
                "{id}: it lands part-way into a lamp (channel {first_channel})"
            ));
        }
        let ports = outputs
            .iter()
            .find(|(node, _)| *node == output.as_str())
            .map(|(_, ports)| *ports)
            .ok_or_else(|| format!("{id}: it lands on {output}, which is not an output"))?;
        let index = ports
            .iter()
            .position(|(key, _)| *key == port)
            .ok_or_else(|| format!("{id}: it lands on port {port} of {output}, which has none"))?;
        let need = segment.lamps.saturating_mul(CHANNELS_PER_LAMP);

        let mut entry = FrozenOverride {
            reverse: reversed,
            offset: rotation,
            ..FrozenOverride::default()
        };
        match &cursor {
            None => {
                frozen.start = Some(FrozenAddress {
                    output: output.clone(),
                    port,
                    lamp: first_channel / CHANNELS_PER_LAMP,
                });
            }
            Some(at) => {
                let follows =
                    at.output == output.as_str() && at.lay(need) == Some((index, first_channel));
                if !follows {
                    if at.output != output.as_str() {
                        entry.output = Some(output.clone());
                    }
                    entry.port = Some(port);
                    entry.lamp = Some(first_channel / CHANNELS_PER_LAMP);
                }
            }
        }
        if entry != FrozenOverride::default() {
            frozen.overrides.push((id.clone(), entry));
        }
        cursor = Some(Cursor {
            output,
            ports,
            port: index,
            channel: first_channel.saturating_add(need),
        });
    }
    Ok(frozen)
}

/// Where the patcher's cursor stands after a laid segment.
struct Cursor<'a> {
    output: &'a str,
    ports: &'a [(u32, Option<u32>)],
    /// The port's index in key order.
    port: usize,
    /// In channels.
    channel: u32,
}

impl Cursor<'_> {
    /// Where the patcher lays the next segment of `need` channels when
    /// nothing moves the cursor: here when it fits, else the next port
    /// from its channel 0 that has room (`None`: past the last port).
    fn lay(&self, need: u32) -> Option<(usize, u32)> {
        if self.room(self.port, self.channel) >= need {
            return Some((self.port, self.channel));
        }
        ((self.port + 1)..self.ports.len())
            .find(|&index| self.room(index, 0) >= need)
            .map(|index| (index, 0))
    }

    /// What is left of port `index` from `channel` (the patcher's `room`).
    fn room(&self, index: usize, channel: u32) -> u32 {
        match self.ports[index].1 {
            Some(lamps) => lamps
                .saturating_mul(CHANNELS_PER_LAMP)
                .saturating_sub(channel),
            None if index + 1 == self.ports.len() => u32::MAX,
            None => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v11::v11_project_landing::DarkReason;
    use crate::v11::v11_segments::SegmentSource;

    const PORTS: &[(u32, Option<u32>)] = &[(0, Some(10)), (1, Some(10)), (2, None)];

    /// A fixture that only flows gets one start and no overrides.
    #[test]
    fn a_fixture_that_only_flows_gets_one_start() {
        let frozen = freeze(&[
            ("a", landed("/o.json", 0, 0, 4)),
            ("b", landed("/o.json", 0, 4, 6)),
        ]);
        assert_eq!(frozen.start, Some(address("/o.json", 0, 0)));
        assert!(frozen.overrides.is_empty(), "{:?}", frozen.overrides);
    }

    /// A segment that doesn't fit what is left of its port and lands on the
    /// next port's first lamp follows on: no override.
    #[test]
    fn a_segment_that_wraps_as_the_patcher_would_needs_no_override() {
        let frozen = freeze(&[
            ("a", landed("/o.json", 0, 0, 7)),
            ("b", landed("/o.json", 1, 0, 6)),
            ("c", landed("/o.json", 2, 0, 40)),
        ]);
        assert!(frozen.overrides.is_empty(), "{:?}", frozen.overrides);
    }

    /// A segment set somewhere else gets port and channel; output only when
    /// it changes.
    #[test]
    fn a_segment_elsewhere_gets_port_and_channel() {
        let frozen = freeze(&[
            ("a", landed("/o.json", 0, 0, 4)),
            ("b", landed("/o.json", 1, 3, 2)),
            ("c", landed("/p.json", 0, 0, 2)),
        ]);
        assert_eq!(
            frozen.overrides,
            vec![
                (
                    String::from("b"),
                    FrozenOverride {
                        port: Some(1),
                        lamp: Some(3),
                        ..FrozenOverride::default()
                    }
                ),
                (
                    String::from("c"),
                    FrozenOverride {
                        output: Some(String::from("/p.json")),
                        port: Some(0),
                        lamp: Some(0),
                        ..FrozenOverride::default()
                    }
                ),
            ]
        );
    }

    /// A dark segment is off and does not move the cursor: what follows it
    /// takes its place.
    #[test]
    fn a_dark_segment_is_off_and_does_not_move_the_cursor() {
        let frozen = freeze(&[
            ("a", dark()),
            ("b", landed("/o.json", 0, 1, 2)),
            ("c", dark()),
            ("d", landed("/o.json", 0, 3, 2)),
        ]);
        assert_eq!(frozen.start, Some(address("/o.json", 0, 1)));
        let ids: Vec<(&str, bool)> = frozen
            .overrides
            .iter()
            .map(|(id, o)| (id.as_str(), o.off))
            .collect();
        assert_eq!(ids, vec![("a", true), ("c", true)]);
    }

    /// Nothing lands: no start, no overrides.
    #[test]
    fn a_fixture_that_lands_nowhere_has_no_start() {
        let frozen = freeze(&[("a", dark()), ("b", dark())]);
        assert_eq!(frozen, FrozenPatching::default());
    }

    /// Reversed and rotated windows carry direction and offset, even where
    /// they follow on.
    #[test]
    fn reversal_and_rotation_are_kept() {
        let mut b = landed("/o.json", 0, 2, 3);
        b.landing = Landing::Landed {
            output: String::from("/o.json"),
            port: 0,
            first_channel: 6,
            reversed: true,
            rotation: 2,
        };
        let frozen = freeze(&[("a", landed("/o.json", 0, 0, 2)), ("b", b)]);
        assert_eq!(
            frozen.overrides,
            vec![(
                String::from("b"),
                FrozenOverride {
                    reverse: true,
                    offset: 2,
                    ..FrozenOverride::default()
                }
            )]
        );
    }

    fn freeze(segments: &[(&str, SegmentLanding)]) -> FrozenPatching {
        let pairs: Vec<(String, &SegmentLanding)> = segments
            .iter()
            .map(|(id, segment)| (id.to_string(), segment))
            .collect();
        freeze_patching(&pairs, &[("/o.json", PORTS), ("/p.json", PORTS)]).unwrap()
    }

    fn landed(output: &str, port: u32, lamp: u32, lamps: u32) -> SegmentLanding {
        SegmentLanding {
            source: SegmentSource::PointList {
                key: 0,
                first_channel: 0,
            },
            first_lamp: 0,
            lamps,
            landing: Landing::Landed {
                output: output.to_string(),
                port,
                first_channel: lamp * 3,
                reversed: false,
                rotation: 0,
            },
        }
    }

    fn dark() -> SegmentLanding {
        SegmentLanding {
            source: SegmentSource::PointList {
                key: 0,
                first_channel: 0,
            },
            first_lamp: 0,
            lamps: 2,
            landing: Landing::Dark {
                why: DarkReason::NotPatched,
            },
        }
    }

    fn address(output: &str, port: u32, lamp: u32) -> FrozenAddress {
        FrozenAddress {
            output: output.to_string(),
            port,
            lamp,
        }
    }
}
