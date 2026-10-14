//! An output: where pixels leave the board, one port per data pin.
//!
//! On the canvas an output is a card, not a frame: clicking it selects the
//! output, and clicking it again goes into the port under the cursor.
//!
//! A port's key is minted by its output (`p1`, `p2`, …), the way a circle
//! mints ring keys, so renaming or deleting one port never moves another —
//! what was patched to `p2` stays on `p2`.

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    /// In data order: what's on the card, top to bottom.
    pub ports: Vec<Port>,
    pub next_port: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Port {
    /// Stable through edits; never shown as a name.
    pub key: String,
    /// What the board calls the pin: `GPIO 10`, `D0`.
    pub name: String,
    /// How many pixels the strip on this pin has.
    pub pixels: u32,
    pub format: PixelFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Rgb,
    Grb,
    Rgbw,
}

impl PixelFormat {
    pub fn word(self) -> &'static str {
        match self {
            PixelFormat::Rgb => "RGB",
            PixelFormat::Grb => "GRB",
            PixelFormat::Rgbw => "RGBW",
        }
    }
}

impl Output {
    /// An output with one port per `(name, pixels, format)`, keys `p1`, `p2`, ….
    pub fn new(ports: &[(&str, u32, PixelFormat)]) -> Self {
        let mut out = Output {
            ports: Vec::new(),
            next_port: 1,
        };
        for &(name, pixels, format) in ports {
            out.add_port(name, pixels, format);
        }
        out
    }

    /// Mint a key and append a port; returns its key.
    pub fn add_port(&mut self, name: &str, pixels: u32, format: PixelFormat) -> String {
        let key = format!("p{}", self.next_port);
        self.next_port += 1;
        self.ports.push(Port {
            key: key.clone(),
            name: name.into(),
            pixels: pixels.max(1),
            format,
        });
        key
    }

    pub fn port(&self, key: &str) -> Option<&Port> {
        self.ports.iter().find(|p| p.key == key)
    }

    pub fn port_mut(&mut self, key: &str) -> Option<&mut Port> {
        self.ports.iter_mut().find(|p| p.key == key)
    }

    pub fn pixels(&self) -> u32 {
        self.ports.iter().map(|p| p.pixels).sum()
    }
}
