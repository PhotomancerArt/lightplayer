//! An output: where pixels leave the board, one port per data pin.
//!
//! On the canvas an output is a card, not a frame: clicking it selects the
//! output, and clicking it again goes into the port under the cursor.
//!
//! A port's key is the product's: a number, minted by its output as one
//! more than the highest it has (`0`, `1`, …), never reused while the port
//! is there — so renaming or deleting one port never moves another: what
//! was patched to port `1` stays on port `1`. Ports run in key order, and
//! together they make one flat wire (the product's numbering: port 0's
//! pixels, then port 1's, …).
//!
//! A port's count is how many pixels its strip has. The last port may have
//! none: it takes the rest — as many as land on it (the product's
//! `OutputPortDef` with no `count`).
//!
//! What a lamp's colour bytes look like (its colour order) belongs to its
//! fixture, as in the product (fit-check §5.3, Q-A); a port card shows
//! which orders land on it.
//!
//! An output reads one control channel (the product's
//! `bindings.input.source`): each fixture writing it arrives as its own
//! control product, and the patch places them on the wire (`board.rs`).

use crate::domain::channel::Channel;

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    /// In key order: what's on the card, top to bottom, and the order
    /// they make the flat wire in.
    pub ports: Vec<Port>,
    /// The product's output name ("Box 2", "1"): how patch documents name
    /// it. Rules here name the node; only saving writes this.
    pub label: Option<String>,
    /// The control channel its fixtures' lamps arrive on.
    pub reads: Channel,
}

impl Default for Output {
    /// No ports, reading `control.out`.
    fn default() -> Self {
        Self {
            ports: Vec::new(),
            label: None,
            reads: Channel::control_out(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Port {
    /// Stable through edits; never shown as a name.
    pub key: String,
    /// What the board calls the pin: `GPIO 10`, `D0`, `ws281x:local:D10`.
    pub name: String,
    /// How many pixels the strip on this pin has; `None` on the last port
    /// means it takes the rest.
    pub pixels: Option<u32>,
}

impl Output {
    /// An output with one port per `(name, pixels)`, keys `0`, `1`, ….
    pub fn new(ports: &[(&str, u32)]) -> Self {
        let mut out = Output::default();
        for &(name, pixels) in ports {
            out.add_port(name, Some(pixels));
        }
        out
    }

    /// Mint a key and append a port; returns its key.
    pub fn add_port(&mut self, name: &str, pixels: Option<u32>) -> String {
        let key = self
            .ports
            .iter()
            .filter_map(|p| p.key.parse::<u32>().ok())
            .max()
            .map_or(0, |k| k + 1)
            .to_string();
        self.ports.push(Port {
            key: key.clone(),
            name: name.into(),
            pixels: pixels.map(|n| n.max(1)),
        });
        key
    }

    pub fn port(&self, key: &str) -> Option<&Port> {
        self.ports.iter().find(|p| p.key == key)
    }

    pub fn port_mut(&mut self, key: &str) -> Option<&mut Port> {
        self.ports.iter_mut().find(|p| p.key == key)
    }

    /// Where a port's pixel 0 sits on the flat wire: the counts of every
    /// port before it. `None` for a port that isn't there.
    pub fn port_start(&self, key: &str) -> Option<u32> {
        let mut start = 0u32;
        for p in &self.ports {
            if p.key == key {
                return Some(start);
            }
            start = start.saturating_add(p.pixels.unwrap_or(0));
        }
        None
    }

    /// The port and pixel a flat wire position lands on, if any port has
    /// it: the last port without a count takes everything past the rest.
    pub fn port_at(&self, wire: u32) -> Option<(usize, u32)> {
        let mut start = 0u32;
        let last = self.ports.len().checked_sub(1)?;
        for (i, p) in self.ports.iter().enumerate() {
            match p.pixels {
                Some(n) if wire < start.saturating_add(n) => return Some((i, wire - start)),
                Some(n) => start = start.saturating_add(n),
                None if i == last => return Some((i, wire - start)),
                None => {}
            }
        }
        None
    }
}
