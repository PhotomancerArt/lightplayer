//! [`EditorDoc`]: the board def being edited, with byte-faithful export.
//!
//! The invariant the round-trip tests pin: **an untouched document exports
//! its loaded bytes verbatim.** Checked-in sidecars are hand-formatted;
//! re-serializing one that was merely opened would churn the file. Only the
//! first edit switches export to the canonical serialization.

use lpa_boards::{BoardDisplayFile, DrawnModule, DrawnPin, PinCap, PinRole};

/// Which pin list an index-based edit targets. Left/right are the header
/// rails; `Terminals` is the top-edge screw-terminal band. All three carry
/// the same editable fields (label, role, gpio, caps), so the pin-table
/// editor serves them uniformly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailTarget {
    Left,
    Right,
    Terminals,
}

impl RailTarget {
    pub fn title(self) -> &'static str {
        match self {
            Self::Left => "Left rail",
            Self::Right => "Right rail",
            Self::Terminals => "Screw terminals",
        }
    }
}

/// One pin row's editable fields, cloned out for rendering. Mutation goes
/// through [`EditorDoc::edit_pin`] so dirty tracking can't be bypassed.
#[derive(Clone, Debug, PartialEq)]
pub struct PinRowData {
    pub label: String,
    pub role: PinRole,
    pub gpio: Option<u8>,
    pub caps: Vec<PinCap>,
}

/// Mutable view over one pin row, uniform across rails and terminals.
pub struct PinFieldsMut<'a> {
    pub label: &'a mut String,
    pub role: &'a mut PinRole,
    pub gpio: &'a mut Option<u8>,
    pub caps: &'a mut Vec<PinCap>,
}

/// The document under edit: the parsed board plus the exact text it was
/// loaded from and whether any edit has happened since.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorDoc {
    pub board: BoardDisplayFile,
    /// The bytes the doc was loaded from — exported verbatim while clean.
    pub source_text: String,
    /// Where the doc came from ("seeed/xiao-esp32-c6", an uploaded filename,
    /// "new board") — display + save-name context only.
    pub source_name: String,
    pub dirty: bool,
}

/// The canonical serialization for edited docs: pretty JSON + trailing
/// newline. Deliberately NOT promised to match hand-formatted sidecars —
/// byte stability for untouched docs comes from keeping the source text.
pub fn canonical_json(board: &BoardDisplayFile) -> String {
    let mut json = serde_json::to_string_pretty(board).expect("board display defs serialize");
    json.push('\n');
    json
}

impl EditorDoc {
    /// Parse a loaded file. Parse errors are load failures; *validity*
    /// problems (duplicate gpios, bad ids) load fine and surface in the lint
    /// panel — fixing broken defs is what the editor is for.
    pub fn from_source(name: &str, text: &str) -> Result<Self, String> {
        let board: BoardDisplayFile =
            serde_json::from_str(text).map_err(|error| format!("{name}: {error}"))?;
        Ok(Self {
            board,
            source_text: text.to_string(),
            source_name: name.to_string(),
            dirty: false,
        })
    }

    /// A fresh minimal template for "New board".
    pub fn new_template() -> Self {
        let board = BoardDisplayFile {
            board_id: "vendor/product".into(),
            display_name: "New board".into(),
            manufacturer: String::new(),
            soc: "ESP32-C6".into(),
            family: "esp32c6".into(),
            flash: "4 MB".into(),
            psram: None,
            price_usd: 0.0,
            tier: lpa_boards::SupportTier::Bronze,
            capabilities: vec![],
            blurb: String::new(),
            support_note: None,
            purchase_urls: vec![],
            usb_bridge: None,
            notes: vec![],
            hw: lpa_boards::BoardDrawing {
                width: 100.0,
                module: DrawnModule {
                    x: 20.0,
                    y: 8.0,
                    w: 60.0,
                    h: 44.0,
                    label: "SOC".into(),
                    antenna: true,
                },
                usb: vec![],
                buttons: vec![],
                rgb: None,
                terminals: vec![],
                left: vec![DrawnPin {
                    label: "4".into(),
                    role: PinRole::Io,
                    gpio: Some(4),
                    caps: vec![],
                }],
                right: vec![],
            },
        };
        Self {
            source_text: canonical_json(&board),
            source_name: "new board".into(),
            dirty: false,
            board,
        }
    }

    /// The text export/save/copy produce right now.
    pub fn export_json(&self) -> String {
        if self.dirty {
            canonical_json(&self.board)
        } else {
            self.source_text.clone()
        }
    }

    /// The download filename: the product stem of the board id.
    pub fn export_file_name(&self) -> String {
        let product = self
            .board
            .board_id
            .rsplit('/')
            .next()
            .filter(|stem| !stem.trim().is_empty())
            .unwrap_or("board");
        format!("{product}.display.json")
    }

    /// Every edit goes through here: mutate the board, mark dirty.
    pub fn edit(&mut self, apply: impl FnOnce(&mut BoardDisplayFile)) {
        apply(&mut self.board);
        self.dirty = true;
    }

    // ---- uniform pin-row access (rails + terminals) ----------------------

    pub fn rail_rows(&self, target: RailTarget) -> Vec<PinRowData> {
        match target {
            RailTarget::Left => self.board.hw.left.iter().map(pin_row).collect(),
            RailTarget::Right => self.board.hw.right.iter().map(pin_row).collect(),
            RailTarget::Terminals => self
                .board
                .hw
                .terminals
                .iter()
                .map(|terminal| PinRowData {
                    label: terminal.label.clone(),
                    role: terminal.role,
                    gpio: terminal.gpio,
                    caps: terminal.caps.clone(),
                })
                .collect(),
        }
    }

    /// Mutate one pin row's fields through the uniform view.
    pub fn edit_pin(
        &mut self,
        target: RailTarget,
        index: usize,
        apply: impl FnOnce(PinFieldsMut<'_>),
    ) {
        self.edit(|board| match target {
            RailTarget::Left => {
                if let Some(pin) = board.hw.left.get_mut(index) {
                    apply(pin_fields(pin));
                }
            }
            RailTarget::Right => {
                if let Some(pin) = board.hw.right.get_mut(index) {
                    apply(pin_fields(pin));
                }
            }
            RailTarget::Terminals => {
                if let Some(terminal) = board.hw.terminals.get_mut(index) {
                    apply(PinFieldsMut {
                        label: &mut terminal.label,
                        role: &mut terminal.role,
                        gpio: &mut terminal.gpio,
                        caps: &mut terminal.caps,
                    });
                }
            }
        });
    }

    /// Append a blank row to the target list.
    pub fn add_pin(&mut self, target: RailTarget) {
        self.edit(|board| match target {
            RailTarget::Left => board.hw.left.push(blank_pin()),
            RailTarget::Right => board.hw.right.push(blank_pin()),
            RailTarget::Terminals => board.hw.terminals.push(lpa_boards::DrawnTerminal {
                label: "T".into(),
                role: PinRole::Io,
                gpio: None,
                caps: vec![],
            }),
        });
    }

    pub fn remove_pin(&mut self, target: RailTarget, index: usize) {
        self.edit(|board| match target {
            RailTarget::Left if index < board.hw.left.len() => {
                board.hw.left.remove(index);
            }
            RailTarget::Right if index < board.hw.right.len() => {
                board.hw.right.remove(index);
            }
            RailTarget::Terminals if index < board.hw.terminals.len() => {
                board.hw.terminals.remove(index);
            }
            _ => {}
        });
    }

    /// Reorder within the list: swap `index` with `index + delta` when both
    /// are in range. Reordering never crosses rails (plan scope).
    pub fn move_pin(&mut self, target: RailTarget, index: usize, delta: isize) {
        let Some(other) = index.checked_add_signed(delta) else {
            return;
        };
        self.edit(|board| match target {
            RailTarget::Left if index < board.hw.left.len() && other < board.hw.left.len() => {
                board.hw.left.swap(index, other);
            }
            RailTarget::Right if index < board.hw.right.len() && other < board.hw.right.len() => {
                board.hw.right.swap(index, other);
            }
            RailTarget::Terminals
                if index < board.hw.terminals.len() && other < board.hw.terminals.len() =>
            {
                board.hw.terminals.swap(index, other);
            }
            _ => {}
        });
    }
}

fn pin_row(pin: &DrawnPin) -> PinRowData {
    PinRowData {
        label: pin.label.clone(),
        role: pin.role,
        gpio: pin.gpio,
        caps: pin.caps.clone(),
    }
}

fn pin_fields(pin: &mut DrawnPin) -> PinFieldsMut<'_> {
    PinFieldsMut {
        label: &mut pin.label,
        role: &mut pin.role,
        gpio: &mut pin.gpio,
        caps: &mut pin.caps,
    }
}

fn blank_pin() -> DrawnPin {
    DrawnPin {
        label: "IO".into(),
        role: PinRole::Io,
        gpio: None,
        caps: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The M4 round-trip gate: load → export is byte-identical for every
    /// untouched checked-in def. Byte stability comes from keeping the
    /// source text, so this also guards against a future "always
    /// re-serialize" regression.
    #[test]
    fn untouched_docs_export_their_source_bytes() {
        for (board_id, source) in lpa_boards::DISPLAY_MANIFEST_SOURCES {
            let doc = EditorDoc::from_source(board_id, source).expect(board_id);
            assert_eq!(doc.export_json(), *source, "{board_id}");
        }
    }

    /// The serializer must lose nothing: canonical JSON parses back to the
    /// same value for every checked-in board (skip_serializing_if fields
    /// included).
    #[test]
    fn canonical_json_round_trips_every_board_by_value() {
        for board in lpa_boards::all_boards() {
            let json = canonical_json(board);
            let back: BoardDisplayFile =
                serde_json::from_str(&json).unwrap_or_else(|error| {
                    panic!("{}: canonical json re-parse: {error}", board.board_id)
                });
            assert_eq!(&back, board, "{}", board.board_id);
        }
    }

    #[test]
    fn first_edit_switches_export_to_canonical() {
        let (board_id, source) = lpa_boards::DISPLAY_MANIFEST_SOURCES[0];
        let mut doc = EditorDoc::from_source(board_id, source).unwrap();
        doc.edit(|board| board.display_name = "Renamed".into());
        assert!(doc.dirty);
        let exported = doc.export_json();
        assert_ne!(exported, source);
        assert_eq!(exported, canonical_json(&doc.board));
        assert!(exported.ends_with('\n'));
    }

    #[test]
    fn pin_rows_and_edits_are_uniform_across_targets() {
        let mut doc = EditorDoc::new_template();
        for target in [RailTarget::Left, RailTarget::Right, RailTarget::Terminals] {
            let before = doc.rail_rows(target).len();
            doc.add_pin(target);
            assert_eq!(doc.rail_rows(target).len(), before + 1, "{target:?}");
            let last = doc.rail_rows(target).len() - 1;
            doc.edit_pin(target, last, |fields| {
                *fields.label = "X1".into();
                *fields.gpio = Some(33);
            });
            let row = &doc.rail_rows(target)[last];
            assert_eq!((row.label.as_str(), row.gpio), ("X1", Some(33)), "{target:?}");
            doc.remove_pin(target, last);
            assert_eq!(doc.rail_rows(target).len(), before, "{target:?}");
        }
    }

    #[test]
    fn move_pin_swaps_within_bounds_only() {
        let mut doc = EditorDoc::new_template();
        doc.add_pin(RailTarget::Left);
        let labels = |doc: &EditorDoc| -> Vec<String> {
            doc.rail_rows(RailTarget::Left)
                .into_iter()
                .map(|row| row.label)
                .collect()
        };
        let before = labels(&doc);
        doc.move_pin(RailTarget::Left, 0, 1);
        let after = labels(&doc);
        assert_eq!(after, vec![before[1].clone(), before[0].clone()]);
        // Out-of-range moves are no-ops.
        doc.move_pin(RailTarget::Left, 1, 1);
        doc.move_pin(RailTarget::Left, 0, -1);
        assert_eq!(labels(&doc), after);
    }

    #[test]
    fn export_file_name_uses_the_product_stem() {
        let doc = EditorDoc::new_template();
        assert_eq!(doc.export_file_name(), "product.display.json");
    }
}
