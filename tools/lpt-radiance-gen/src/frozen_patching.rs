//! The placeholder patch as map2d format 6's patching: the fixture's start
//! and an override on every panel copy that does not simply follow on.
//!
//! The rule is the format-12 upgrade's (`lpa-upgrade`'s
//! `v11/freeze_patching.rs`), re-stated here so the generator writes the
//! bytes the upgrade wrote for this project. The segments are walked in
//! order with a **predicted cursor** that moves as the patcher's does, on
//! ports of `count × 3` channels:
//!
//! - **The start** is the first segment's address: output Set (by name),
//!   port Set, channel Set in lamps.
//! - **A segment whose address is where the cursor would lay it** — at the
//!   cursor, or on the next port with room from its channel 0 when it does
//!   not fit what is left — gets no override for its address. Any other
//!   gets port Set and channel Set (in lamps), and output Set only when its
//!   output is not the cursor's.
//! - Reversed adds `direction: reverse`; a rotation `k` adds `offset: k`.
//! - The cursor then sits at the end of the segment's true address.
//!
//! The plan names every panel copy, so no segment is dark here: the
//! upgrade's `off` overrides never come up.

use std::collections::BTreeMap;

use lpc_mapping::part::TreeId;
use lpc_mapping::patching::{
    Amount, ChannelOp, Direction, OutputName, OutputOp, Override, PortOp, Start,
};

use crate::PORTS_PER_OUTPUT;
use crate::patch_plan::{PatchRow, output_name};

/// Channels per lamp: the dome is RGB.
const CHANNELS_PER_LAMP: u32 = 3;

/// A fixture's patching as format 6 writes it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FrozenPatching {
    pub start: Start,
    pub overrides: BTreeMap<TreeId, Override>,
}

/// Freeze the plan's landing of each segment. `segments` pairs each
/// segment's id and lamp count with its row, in the fixture's lamp order;
/// `port_lamps` is every port's capacity in lamps (all ports alike).
pub fn freeze_patching(segments: &[(TreeId, u32, &PatchRow)], port_lamps: u32) -> FrozenPatching {
    let mut frozen = FrozenPatching::default();
    let mut cursor: Option<Cursor> = None;
    for (id, lamps, row) in segments {
        let need = lamps * CHANNELS_PER_LAMP;
        // Every row lands on its port's first lamp.
        let (port, first_channel) = (row.port, 0);
        let mut entry = Override {
            direction: row.reversed.then_some(Direction::Reverse),
            offset: (row.offset != 0).then_some(row.offset),
            ..Override::default()
        };
        match &cursor {
            None => {
                frozen.start = Start::at(
                    &output_name(row.output),
                    port as u32,
                    Amount::lamps(first_channel / CHANNELS_PER_LAMP),
                );
            }
            Some(at) => {
                let follows =
                    at.output == row.output && at.lay(need) == Some((port, first_channel));
                if !follows {
                    if at.output != row.output {
                        entry.output =
                            Some(OutputOp::Set(OutputName::new(&output_name(row.output))));
                    }
                    entry.port = Some(PortOp::set(port as u32));
                    entry.channel = Some(ChannelOp::Set(Amount::lamps(
                        first_channel / CHANNELS_PER_LAMP,
                    )));
                }
            }
        }
        if !entry.is_empty() {
            frozen.overrides.insert(id.clone(), entry);
        }
        cursor = Some(Cursor {
            output: row.output,
            port,
            channel: first_channel + need,
            port_channels: port_lamps * CHANNELS_PER_LAMP,
        });
    }
    frozen
}

/// Where the patcher's cursor stands after a laid segment.
struct Cursor {
    /// 0-based output index.
    output: usize,
    port: usize,
    /// In channels.
    channel: u32,
    /// Every port's capacity, in channels.
    port_channels: u32,
}

impl Cursor {
    /// Where the patcher lays the next segment of `need` channels when
    /// nothing moves the cursor: here when it fits, else the next port from
    /// its channel 0 that has room (`None`: past the last port).
    fn lay(&self, need: u32) -> Option<(usize, u32)> {
        if self.port_channels.saturating_sub(self.channel) >= need {
            return Some((self.port, self.channel));
        }
        // Every port is alike, so the next port has room or none does.
        let next = self.port + 1;
        (next < PORTS_PER_OUTPUT && self.port_channels >= need).then_some((next, 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_mapping::part::PartId;

    /// Rows that follow on, port after port, get one start and no
    /// overrides.
    #[test]
    fn rows_that_follow_on_get_one_start() {
        let rows = [row(0, 0), row(0, 1), row(0, 2)];
        let frozen = freeze(&rows);
        assert_eq!(frozen.start, Start::at("1", 0, Amount::lamps(0)));
        assert!(frozen.overrides.is_empty(), "{:?}", frozen.overrides);
    }

    /// A row elsewhere gets port and channel; output only when it changes.
    #[test]
    fn a_row_elsewhere_gets_port_and_channel() {
        let rows = [row(0, 0), row(0, 5), row(2, 5)];
        let frozen = freeze(&rows);
        let set = |output: Option<&str>, port: u32| Override {
            output: output.map(|o| OutputOp::Set(OutputName::new(o))),
            port: Some(PortOp::set(port)),
            channel: Some(ChannelOp::Set(Amount::lamps(0))),
            ..Override::default()
        };
        assert_eq!(
            frozen.overrides.into_iter().collect::<Vec<_>>(),
            vec![(id(1), set(None, 5)), (id(2), set(Some("3"), 5))]
        );
    }

    /// Reversal and rotation are kept even where a row follows on.
    #[test]
    fn reversal_and_rotation_are_kept() {
        let mut b = row(0, 1);
        b.reversed = true;
        b.offset = 7;
        let frozen = freeze(&[row(0, 0), b]);
        assert_eq!(
            frozen.overrides.into_iter().collect::<Vec<_>>(),
            vec![(
                id(1),
                Override {
                    direction: Some(Direction::Reverse),
                    offset: Some(7),
                    ..Override::default()
                }
            )]
        );
    }

    fn freeze(rows: &[PatchRow]) -> FrozenPatching {
        let segments: Vec<(TreeId, u32, &PatchRow)> = rows
            .iter()
            .enumerate()
            .map(|(k, row)| (id(k), 80, row))
            .collect();
        freeze_patching(&segments, 80)
    }

    fn id(k: usize) -> TreeId {
        TreeId::of(&PartId::new(&format!("s{k}")).unwrap())
    }

    fn row(output: usize, port: usize) -> PatchRow {
        PatchRow {
            id: String::from("s"),
            sector: 0,
            output,
            port,
            reversed: false,
            offset: 0,
        }
    }
}
