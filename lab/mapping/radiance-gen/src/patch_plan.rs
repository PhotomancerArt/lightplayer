//! The placeholder patch: one panel per port, in sector order.
//!
//! Panel `n = sector·55 + class` goes to output `n / 32` (named `"1"` to
//! `"10"`), port `n % 32`, wire lamp `port·80`. 275 panels fill outputs
//! 1–8 and 19 ports of output 9; the rest of 9 and all of 10 stay empty.
//! fit-check Q-E's lean, until the as-built patch is found.
//!
//! Two invented install quirks keep it from being a pure identity, the way
//! `lpt-geodome`'s small dome has them: one panel plugged in reversed and
//! one seated a third of the way round its own strip.

use crate::dome_geometry::{CLASSES, SECTORS};
use crate::{OUTPUTS, PANEL_LAMPS, PORTS_PER_OUTPUT};

/// The panel seated reversed: class id and sector. Ring 6 is the first
/// full-width band below the cap; sector 2 sits on output "5".
pub const REVERSED_PANEL: (&str, usize) = ("r6-p04", 2);
/// The panel seated rotated, its class id and sector, and the rotation in
/// lamps (a third of the 80-lamp strip, rounded).
pub const ROTATED_PANEL: (&str, usize, u32) = ("r7-p05", 3, 27);

/// One patch row: a panel instance and where its run lands.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchRow {
    /// The panel class's object id.
    pub id: String,
    pub sector: usize,
    /// 0-based output index; the output's name is `output + 1`.
    pub output: usize,
    pub port: usize,
    /// Wire anchor in lamps on that output.
    pub wire: u32,
    pub reversed: bool,
    /// Rotation within the run, in lamps.
    pub offset: u32,
}

/// Every panel's row, in panel order. `class_ids[class]` is each class's
/// object id.
pub fn patch_rows(class_ids: &[String]) -> Vec<PatchRow> {
    assert_eq!(class_ids.len(), CLASSES);
    assert!(SECTORS * CLASSES <= OUTPUTS * PORTS_PER_OUTPUT);
    (0..SECTORS)
        .flat_map(|sector| (0..CLASSES).map(move |class| (sector, class)))
        .map(|(sector, class)| {
            let panel = sector * CLASSES + class;
            let port = panel % PORTS_PER_OUTPUT;
            let id = class_ids[class].clone();
            let is = |quirk_id: &str, quirk_sector: usize| id == quirk_id && sector == quirk_sector;
            PatchRow {
                reversed: is(REVERSED_PANEL.0, REVERSED_PANEL.1),
                offset: if is(ROTATED_PANEL.0, ROTATED_PANEL.1) {
                    ROTATED_PANEL.2
                } else {
                    0
                },
                id,
                sector,
                output: panel / PORTS_PER_OUTPUT,
                port,
                wire: port as u32 * PANEL_LAMPS,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> Vec<String> {
        // The real ids come from the dome; any 55 distinct ones that include
        // the quirk ids exercise the plan.
        let mut ids: Vec<String> = (0..CLASSES).map(|c| format!("c{c}")).collect();
        ids[28] = REVERSED_PANEL.0.to_string();
        ids[39] = ROTATED_PANEL.0.to_string();
        ids
    }

    #[test]
    fn one_panel_per_port_in_sector_order() {
        let rows = patch_rows(&ids());
        assert_eq!(rows.len(), 275);
        assert_eq!((rows[0].output, rows[0].port, rows[0].wire), (0, 0, 0));
        assert_eq!((rows[255].output, rows[255].port), (7, 31));
        let last = rows.last().unwrap();
        assert_eq!((last.output, last.port, last.wire), (8, 18, 18 * 80));
    }

    #[test]
    fn exactly_one_reversed_and_one_rotated_row() {
        let rows = patch_rows(&ids());
        let reversed: Vec<_> = rows.iter().filter(|r| r.reversed).collect();
        let rotated: Vec<_> = rows.iter().filter(|r| r.offset != 0).collect();
        assert_eq!(reversed.len(), 1);
        assert_eq!(rotated.len(), 1);
        assert_eq!(
            (reversed[0].id.as_str(), reversed[0].sector),
            REVERSED_PANEL
        );
        assert_eq!(rotated[0].offset, ROTATED_PANEL.2);
    }
}
