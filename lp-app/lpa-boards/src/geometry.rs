//! Row-engine layout for board diagrams: every drawn coordinate in one place.
//!
//! The layout unit is `u` — the pin pitch, which is also the row height. Every
//! other dimension derives from it (see `docs/design/board-diagrams.md`), so
//! diagrams stay dense and collision-free at any scale. The renderer
//! ([`crate::diagram::BoardDiagram`]) walks the computed [`BoardLayout`]
//! without doing geometry of its own, and stories reuse the same layout to
//! anchor annotations — the engine is deterministic, so computed positions
//! ARE the rendered positions.
//!
//! Ported from the approved UX spike (`spikes/hardware-boards/index.html`,
//! rev 5, PR #222). Deviations from the spike are called out inline.

use crate::display_manifest::{BoardDisplayFile, CapKind, PinCap, PinRole};

/// Cell height as a fraction of `u`.
pub const CELL_HEIGHT_U: f32 = 0.78;
/// Pad width as a fraction of `u`.
pub const PAD_WIDTH_U: f32 = 0.62;
/// Pad height as a fraction of `u`.
pub const PAD_HEIGHT_U: f32 = 0.45;
/// Cell font size as a fraction of `u`.
pub const FONT_SIZE_U: f32 = 0.5;
/// Gap between cells in a row as a fraction of `u`.
pub const CELL_GAP_U: f32 = 0.22;

/// What a diagram is answering. See the design language doc for where each
/// mode is used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiagramMode {
    /// What does the board look like? (catalog / picker thumbnails)
    #[default]
    Plain,
    /// What's supported on each pin? (boards page detail)
    Caps,
    /// What's connected? (device card hardware pane)
    Wired,
    /// Which pin are my LEDs on? (pin discovery)
    Swatch,
}

/// One bound connection shown in [`DiagramMode::Wired`]. Bound = violet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WiredConnection {
    pub gpio: u8,
    /// The connection's name ("porch strip").
    pub title: String,
    /// Optional second cell ("WS2812 ×300").
    pub extra: Option<String>,
}

/// One pin's discovery color code shown in [`DiagramMode::Swatch`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinSwatch {
    pub gpio: u8,
    /// CSS colors, one per displayed pixel (separators included).
    pub colors: Vec<String>,
    /// Highlight as the confirmed pin.
    pub selected: bool,
}

/// Layout inputs. `scale` is not here on purpose: it only multiplies the SVG's
/// on-screen size, never the geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct DiagramOptions {
    pub mode: DiagramMode,
    /// Pin pitch in drawing units. The spike settled on 12 (compact contexts)
    /// and 13 (detail contexts).
    pub u: f32,
    /// Draw pin labels and hardware captions. Thumbnails turn this off.
    pub labels: bool,
    /// Connections consulted in [`DiagramMode::Wired`].
    pub wired: Vec<WiredConnection>,
    /// Codes consulted in [`DiagramMode::Swatch`].
    pub swatches: Vec<PinSwatch>,
}

impl Default for DiagramOptions {
    fn default() -> Self {
        Self {
            mode: DiagramMode::Plain,
            u: 13.0,
            labels: true,
            wired: Vec::new(),
            swatches: Vec::new(),
        }
    }
}

/// Visual family of a cell (or of a pin label in non-plain modes). Maps 1:1
/// onto the `lpb-cell--*` / `lpb-fg--*` CSS classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellKind {
    Name,
    Pwr,
    Gnd,
    Ctl,
    Adc,
    Dac,
    Touch,
    Spi,
    I2c,
    Uart,
    Usb,
    Strap,
    Warn,
    Note,
    /// A bound connection — violet, per the studio convention.
    Conn,
}

impl CellKind {
    /// CSS class suffix (`lpb-cell--{suffix}`).
    pub fn css_suffix(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Pwr => "pwr",
            Self::Gnd => "gnd",
            Self::Ctl => "ctl",
            Self::Adc => "adc",
            Self::Dac => "dac",
            Self::Touch => "touch",
            Self::Spi => "spi",
            Self::I2c => "i2c",
            Self::Uart => "uart",
            Self::Usb => "usb",
            Self::Strap => "strap",
            Self::Warn => "warn",
            Self::Note => "note",
            Self::Conn => "conn",
        }
    }
}

impl From<CapKind> for CellKind {
    fn from(kind: CapKind) -> Self {
        match kind {
            CapKind::Adc => Self::Adc,
            CapKind::Dac => Self::Dac,
            CapKind::Touch => Self::Touch,
            CapKind::Spi => Self::Spi,
            CapKind::I2c => Self::I2c,
            CapKind::Uart => Self::Uart,
            CapKind::Usb => Self::Usb,
            CapKind::Strap => Self::Strap,
            CapKind::Pwr => Self::Pwr,
            CapKind::Warn => Self::Warn,
            CapKind::Note => Self::Note,
        }
    }
}

/// An axis-aligned rectangle in board coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
}

/// What a cell displays.
#[derive(Clone, Debug, PartialEq)]
pub enum CellBody {
    Text { text: String, kind: CellKind },
    Swatch { colors: Vec<String>, selected: bool },
}

/// One positioned cell in a row.
#[derive(Clone, Debug, PartialEq)]
pub struct CellLayout {
    pub rect: Rect,
    pub body: CellBody,
}

/// A pin's label. On rails it sits inside the board (silkscreen-style) so the
/// outside cells share one start edge; on band rows the name is a leading
/// cell instead and this is absent.
#[derive(Clone, Debug, PartialEq)]
pub struct RowLabel {
    pub text: String,
    pub x: f32,
    /// Text baseline y.
    pub y: f32,
    /// `true` = `text-anchor: start` (left rail), else `end` (right rail).
    pub start_anchored: bool,
    /// `None` = plain-mode silkscreen color; `Some` = the name-cell color
    /// family for the pin's role.
    pub kind: Option<CellKind>,
    pub font_size: f32,
}

/// One rail pin's row: pad on the board edge, label inside, cells outside.
#[derive(Clone, Debug, PartialEq)]
pub struct RailRow {
    pub gpio: Option<u8>,
    pub role: PinRole,
    /// Row centerline.
    pub y: f32,
    pub pad: Rect,
    pub label: Option<RowLabel>,
    pub cells: Vec<CellLayout>,
}

/// A top-edge terminal's row in the band above the board, tied to its pad by
/// a leader line.
#[derive(Clone, Debug, PartialEq)]
pub struct BandRow {
    pub gpio: Option<u8>,
    pub role: PinRole,
    /// Row centerline (negative: above the board).
    pub y: f32,
    /// The leader's vertical x (the terminal pad center).
    pub pad_x: f32,
    /// Elbow polyline: down from the pad, across to the row.
    pub leader: [(f32, f32); 3],
    pub cells: Vec<CellLayout>,
}

/// A screw terminal's hardware footprint on the board's top edge.
#[derive(Clone, Debug, PartialEq)]
pub struct TerminalBlock {
    pub rect: Rect,
    pub screw_center: (f32, f32),
    pub screw_radius: f32,
    /// Present in plain mode with labels on (band rows carry the name
    /// otherwise).
    pub label: Option<RowLabel>,
}

/// The fully resolved diagram geometry. All coordinates are board-local
/// drawing units; the board's top-left corner is the origin, so rails extend
/// into negative x and the band into negative y.
#[derive(Clone, Debug, PartialEq)]
pub struct BoardLayout {
    pub mode: DiagramMode,
    pub u: f32,
    pub cell_h: f32,
    pub font: f32,
    pub pad_w: f32,
    pub pad_h: f32,
    pub gap: f32,
    /// PCB outline size.
    pub board_w: f32,
    pub board_h: f32,
    /// y where the first pin row starts.
    pub header_top: f32,
    /// `[x, y, w, h]`.
    pub view_box: [f32; 4],
    pub left: Vec<RailRow>,
    pub right: Vec<RailRow>,
    pub band: Vec<BandRow>,
    pub terminals: Vec<TerminalBlock>,
}

impl BoardLayout {
    pub fn compute(board: &BoardDisplayFile, opts: &DiagramOptions) -> Self {
        let hw = &board.hw;
        let u = opts.u;
        let cell_h = CELL_HEIGHT_U * u;
        let font = FONT_SIZE_U * u;
        let pad_w = PAD_WIDTH_U * u;
        let pad_h = PAD_HEIGHT_U * u;
        let gap = CELL_GAP_U * u;
        let label_font = font.min(6.5);

        let header_top = hw.module.y + hw.module.h + 0.8 * u;
        let n_max = hw.left.len().max(hw.right.len()) as f32;
        let board_w = hw.width;
        let board_h = header_top
            + n_max * u
            + if hw.buttons.is_empty() { 0.8 * u } else { 2.2 * u };

        let cell_width = |body: &CellBody| -> f32 {
            match body {
                CellBody::Swatch { colors, .. } => colors.len() as f32 * (cell_h * 0.74) + 8.0,
                CellBody::Text { text, .. } => {
                    (text.chars().count() as f32 * font * 0.66 + 9.0).max(18.0)
                }
            }
        };
        let row_width = |bodies: &[CellBody]| -> f32 {
            bodies.iter().map(cell_width).sum::<f32>()
                + gap * (bodies.len().saturating_sub(1)) as f32
        };
        let cells_for = |label: &str, role: PinRole, gpio: Option<u8>, caps: &[PinCap], with_name: bool| {
            cell_bodies(opts, label, role, gpio, caps, with_name)
        };

        // Rails: rows extend outward from the pads; cells run away from the
        // board so every pin's row can hold cells simultaneously.
        let mut rails: [Vec<RailRow>; 2] = [Vec::new(), Vec::new()];
        let mut rail_extents = [6.0f32, 6.0f32];
        for (side, pins) in [(0usize, &hw.left), (1usize, &hw.right)] {
            for (index, pin) in pins.iter().enumerate() {
                let y = header_top + index as f32 * u + u / 2.0;
                let bodies = cells_for(&pin.label, pin.role, pin.gpio, &pin.caps, false);
                rail_extents[side] = rail_extents[side].max(row_width(&bodies) + 8.0);
                let px = if side == 0 { 1.0 } else { board_w - 1.0 - pad_w };
                let pad = Rect {
                    x: px,
                    y: y - pad_h / 2.0,
                    w: pad_w,
                    h: pad_h,
                };
                let label = opts.labels.then(|| {
                    let (text, kind) = match opts.mode {
                        DiagramMode::Plain => (pin.label.clone(), None),
                        _ => (
                            name_cell_text(&pin.label, pin.role),
                            Some(name_cell_kind(pin.role)),
                        ),
                    };
                    RowLabel {
                        text,
                        x: if side == 0 { px + pad_w + 3.0 } else { px - 3.0 },
                        y: y + font * 0.36,
                        start_anchored: side == 0,
                        kind,
                        font_size: label_font,
                    }
                });
                let (start_x, dir) = if side == 0 {
                    (-4.0, -1.0)
                } else {
                    (board_w + 4.0, 1.0)
                };
                let cells = place_cells(bodies, y, start_x, dir, cell_h, gap, &cell_width);
                rails[side].push(RailRow {
                    gpio: pin.gpio,
                    role: pin.role,
                    y,
                    pad,
                    label,
                    cells,
                });
            }
        }
        let [left, right] = rails;
        let [cw_left, cw_right] = rail_extents;

        // Terminal hardware blocks along the top edge.
        let term_w = 20.0;
        let term_x0 = (board_w - hw.terminals.len() as f32 * term_w) / 2.0;
        let terminals: Vec<TerminalBlock> = hw
            .terminals
            .iter()
            .enumerate()
            .map(|(index, terminal)| {
                let tx = term_x0 + index as f32 * term_w;
                let center_x = tx + (term_w - 2.0) / 2.0;
                let label = (opts.mode == DiagramMode::Plain && opts.labels).then(|| RowLabel {
                    text: terminal.label.clone(),
                    x: center_x,
                    y: -7.0,
                    start_anchored: false,
                    kind: None,
                    font_size: label_font,
                });
                TerminalBlock {
                    rect: Rect {
                        x: tx,
                        y: -2.0,
                        w: term_w - 2.0,
                        h: 16.0,
                    },
                    screw_center: (center_x, 6.0),
                    screw_radius: 4.4,
                    label,
                }
            })
            .collect();

        // Band rows for the terminals, stacked above the board: the deepest
        // terminal row belongs to the first terminal.
        let band: Vec<BandRow> = if opts.mode == DiagramMode::Plain {
            Vec::new()
        } else {
            hw.terminals
                .iter()
                .enumerate()
                .map(|(index, terminal)| {
                    let bodies = cells_for(
                        &terminal.label,
                        terminal.role,
                        terminal.gpio,
                        &terminal.caps,
                        true,
                    );
                    let pad_x = term_x0 + index as f32 * term_w + (term_w - 2.0) / 2.0;
                    let y = -(1.6 * u) - (hw.terminals.len() - 1 - index) as f32 * u;
                    let cells =
                        place_cells(bodies, y, pad_x + 8.0, 1.0, cell_h, gap, &cell_width);
                    BandRow {
                        gpio: terminal.gpio,
                        role: terminal.role,
                        y,
                        pad_x,
                        leader: [(pad_x, -3.0), (pad_x, y), (pad_x + 6.0, y)],
                        cells,
                    }
                })
                .collect()
        };

        let band_top = if band.is_empty() {
            0.0
        } else {
            band.iter().map(|row| row.y).fold(f32::INFINITY, f32::min) - u / 2.0
        };
        // Band rows always lead with a name cell, so `last()` is the row's
        // outer edge; +2 reproduces the spike's `padX + 10 + rowWidth`.
        let band_right = band
            .iter()
            .map(|row| row.cells.last().map_or(0.0, |cell| cell.rect.right() + 2.0))
            .fold(0.0f32, f32::max);
        let term_caption_h: f32 = if !terminals.is_empty() && opts.mode == DiagramMode::Plain {
            18.0
        } else {
            0.0
        };

        let vb_x = -cw_left;
        let vb_y = (-4.0 - term_caption_h).min(band_top - 4.0);
        let vb_w = (board_w + cw_left + cw_right).max(band_right + cw_left + 6.0);
        let vb_h = board_h + 1.6 * u - vb_y;

        Self {
            mode: opts.mode,
            u,
            cell_h,
            font,
            pad_w,
            pad_h,
            gap,
            board_w,
            board_h,
            header_top,
            view_box: [vb_x, vb_y, vb_w, vb_h],
            left,
            right,
            band,
            terminals,
        }
    }

    /// All rail rows, left then right — the pad order the spike's anatomy
    /// figure indexed by.
    pub fn rail_rows(&self) -> impl Iterator<Item = &RailRow> {
        self.left.iter().chain(self.right.iter())
    }
}

/// The display text of a pin's name: numeric GPIO silkscreen labels gain an
/// `IO` prefix outside plain mode ("4" → "IO4"); named pins keep their label.
pub fn name_cell_text(label: &str, role: PinRole) -> String {
    let io_ish = matches!(
        role,
        PinRole::Io | PinRole::IoIn | PinRole::Strap | PinRole::Usb | PinRole::Rsvd
    );
    if io_ish && !label.is_empty() && label.bytes().all(|byte| byte.is_ascii_digit()) {
        format!("IO{label}")
    } else {
        label.to_string()
    }
}

/// The color family of a pin's name cell / label, by role.
pub fn name_cell_kind(role: PinRole) -> CellKind {
    match role {
        PinRole::Pwr5 | PinRole::Pwr3 => CellKind::Pwr,
        PinRole::Gnd | PinRole::Nc => CellKind::Gnd,
        PinRole::Ctl => CellKind::Ctl,
        PinRole::Io | PinRole::IoIn | PinRole::Strap | PinRole::Usb | PinRole::Rsvd => {
            CellKind::Name
        }
    }
}

/// CSS class suffix for a pad's role fill (`lpb-pad--{suffix}`).
pub fn pad_css_suffix(role: PinRole) -> &'static str {
    match role {
        PinRole::Pwr5 => "pwr5",
        PinRole::Pwr3 => "pwr3",
        PinRole::Gnd => "gnd",
        PinRole::Io => "io",
        PinRole::IoIn => "ioin",
        PinRole::Strap => "strap",
        PinRole::Usb => "usb",
        PinRole::Ctl => "ctl",
        PinRole::Nc => "nc",
        // Not in the spike (M1 added the role): reserved pins take the muted
        // input-only fill — present but visually recessive.
        PinRole::Rsvd => "ioin",
    }
}

/// The cells a pin shows in the current mode. Band rows pass
/// `with_name = true` — they have no inside-the-board space for a label, so
/// the name leads the row as a cell.
fn cell_bodies(
    opts: &DiagramOptions,
    label: &str,
    role: PinRole,
    gpio: Option<u8>,
    caps: &[PinCap],
    with_name: bool,
) -> Vec<CellBody> {
    if opts.mode == DiagramMode::Plain {
        return Vec::new();
    }
    let mut bodies = Vec::new();
    if with_name {
        bodies.push(CellBody::Text {
            text: name_cell_text(label, role),
            kind: name_cell_kind(role),
        });
    }
    match opts.mode {
        DiagramMode::Plain => {}
        DiagramMode::Caps => {
            bodies.extend(caps.iter().map(|cap| CellBody::Text {
                text: cap.text.clone(),
                kind: cap.kind.into(),
            }));
        }
        DiagramMode::Wired => {
            if let Some(connection) = gpio
                .and_then(|gpio| opts.wired.iter().find(|wired| wired.gpio == gpio))
            {
                bodies.push(CellBody::Text {
                    text: connection.title.clone(),
                    kind: CellKind::Conn,
                });
                if let Some(extra) = &connection.extra {
                    bodies.push(CellBody::Text {
                        text: extra.clone(),
                        kind: CellKind::Conn,
                    });
                }
            }
        }
        DiagramMode::Swatch => {
            if let Some(swatch) = gpio
                .and_then(|gpio| opts.swatches.iter().find(|swatch| swatch.gpio == gpio))
            {
                bodies.push(CellBody::Swatch {
                    colors: swatch.colors.clone(),
                    selected: swatch.selected,
                });
            }
        }
    }
    bodies
}

/// Position a row's cells from `start_x`, running in `dir` (+1 rightward for
/// right rail and band, −1 leftward for the left rail).
fn place_cells(
    bodies: Vec<CellBody>,
    y: f32,
    start_x: f32,
    dir: f32,
    cell_h: f32,
    gap: f32,
    cell_width: &impl Fn(&CellBody) -> f32,
) -> Vec<CellLayout> {
    let mut x = start_x;
    bodies
        .into_iter()
        .map(|body| {
            let w = cell_width(&body);
            let cx = if dir > 0.0 { x } else { x - w };
            x += dir * (w + gap);
            CellLayout {
                rect: Rect {
                    x: cx,
                    y: y - cell_h / 2.0,
                    w,
                    h: cell_h,
                },
                body,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::board_by_id;

    fn quinled() -> &'static BoardDisplayFile {
        board_by_id("quinled/dig-uno").expect("quinled sidecar embedded")
    }

    fn c6() -> &'static BoardDisplayFile {
        board_by_id("espressif/esp32-c6-devkitc-1").expect("c6 sidecar embedded")
    }

    #[test]
    fn row_pitch_is_u() {
        let layout = BoardLayout::compute(
            c6(),
            &DiagramOptions {
                mode: DiagramMode::Caps,
                ..DiagramOptions::default()
            },
        );
        for rail in [&layout.left, &layout.right] {
            for pair in rail.windows(2) {
                assert!((pair[1].y - pair[0].y - layout.u).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn derived_sizes_follow_u() {
        for u in [12.0, 13.0] {
            let layout = BoardLayout::compute(
                c6(),
                &DiagramOptions {
                    mode: DiagramMode::Caps,
                    u,
                    ..DiagramOptions::default()
                },
            );
            assert!((layout.cell_h - CELL_HEIGHT_U * u).abs() < 1e-4);
            assert!((layout.pad_w - PAD_WIDTH_U * u).abs() < 1e-4);
            assert!((layout.font - FONT_SIZE_U * u).abs() < 1e-4);
            assert!((layout.gap - CELL_GAP_U * u).abs() < 1e-4);
        }
    }

    #[test]
    fn view_box_contains_all_rows_and_band() {
        let layout = BoardLayout::compute(
            quinled(),
            &DiagramOptions {
                mode: DiagramMode::Caps,
                ..DiagramOptions::default()
            },
        );
        let [vx, vy, vw, vh] = layout.view_box;
        assert!(!layout.band.is_empty(), "quinled has terminal band rows");
        for row in layout.rail_rows() {
            for cell in &row.cells {
                assert!(cell.rect.x >= vx && cell.rect.right() <= vx + vw);
                assert!(cell.rect.y >= vy && cell.rect.bottom() <= vy + vh);
            }
        }
        for row in &layout.band {
            assert!(row.y < 0.0, "band rows sit above the board");
            for cell in &row.cells {
                assert!(cell.rect.x >= vx && cell.rect.right() <= vx + vw);
                assert!(cell.rect.y >= vy && cell.rect.bottom() <= vy + vh);
            }
        }
    }

    #[test]
    fn plain_mode_has_no_cells_or_band() {
        let layout = BoardLayout::compute(quinled(), &DiagramOptions::default());
        assert!(layout.band.is_empty());
        assert!(layout.rail_rows().all(|row| row.cells.is_empty()));
        // Terminal hardware still draws, with plain-mode caption labels.
        assert_eq!(layout.terminals.len(), 4);
        assert!(layout.terminals.iter().all(|term| term.label.is_some()));
    }

    #[test]
    fn wired_mode_adds_conn_cells_only_for_matched_gpios() {
        let layout = BoardLayout::compute(
            c6(),
            &DiagramOptions {
                mode: DiagramMode::Wired,
                wired: vec![WiredConnection {
                    gpio: 4,
                    title: "porch strip".into(),
                    extra: Some("WS2812 ×300".into()),
                }],
                ..DiagramOptions::default()
            },
        );
        let wired_row = layout
            .rail_rows()
            .find(|row| row.gpio == Some(4))
            .expect("gpio 4 on the c6 devkit");
        assert_eq!(wired_row.cells.len(), 2);
        assert!(wired_row.cells.iter().all(|cell| matches!(
            &cell.body,
            CellBody::Text { kind: CellKind::Conn, .. }
        )));
        let unwired = layout.rail_rows().find(|row| row.gpio == Some(5)).unwrap();
        assert!(unwired.cells.is_empty());
    }

    #[test]
    fn left_rail_cells_run_leftward_from_shared_start_edge() {
        let layout = BoardLayout::compute(
            c6(),
            &DiagramOptions {
                mode: DiagramMode::Caps,
                ..DiagramOptions::default()
            },
        );
        for row in &layout.left {
            if let Some(first) = row.cells.first() {
                assert!((first.rect.right() - -4.0).abs() < 1e-3);
            }
        }
        for row in &layout.right {
            if let Some(first) = row.cells.first() {
                assert!((first.rect.x - (layout.board_w + 4.0)).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn numeric_gpio_labels_gain_io_prefix() {
        assert_eq!(name_cell_text("4", PinRole::Io), "IO4");
        assert_eq!(name_cell_text("35", PinRole::Rsvd), "IO35");
        assert_eq!(name_cell_text("D10", PinRole::Io), "D10");
        assert_eq!(name_cell_text("3V3", PinRole::Pwr3), "3V3");
    }
}
