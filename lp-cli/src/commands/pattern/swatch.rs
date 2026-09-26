//! A swatch: a standard lamp shape a pattern is previewed on — one
//! `*.map2d.json`, named by its file stem (`choker`, `matrix16`, …).
//!
//! The mapping is resolved here once so the record can carry the lamp
//! positions in wiring order, which is also the order the output publishes
//! their colours in.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use lpc_mapping::{Map2dDoc, resolve};

/// One swatch, loaded and resolved.
#[derive(Debug, Clone)]
pub struct Swatch {
    /// File stem without `.map2d`, e.g. `ring24`.
    pub name: String,
    /// The mapping document, verbatim (it is copied into the rig).
    pub map_json: String,
    /// Lamp positions in doc space, wiring order (doc y runs DOWN, SVG-style).
    pub positions: Vec<[f32; 2]>,
}

impl Swatch {
    pub fn read(path: &Path) -> Result<Self> {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .with_context(|| format!("{}: no file name", path.display()))?;
        let name = file_name
            .strip_suffix(".map2d.json")
            .or_else(|| file_name.strip_suffix(".json"))
            .unwrap_or(file_name)
            .to_string();
        let map_json =
            fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let doc = Map2dDoc::from_json(&map_json)
            .map_err(|e| anyhow::anyhow!("{}: not a map2d document: {e:?}", path.display()))?;
        let positions = resolve(&doc)
            .map_err(|e| anyhow::anyhow!("{}: mapping does not resolve: {e:?}", path.display()))?
            .positions();
        if positions.is_empty() {
            bail!("{}: mapping has no lamps", path.display());
        }
        Ok(Self {
            name,
            map_json,
            positions,
        })
    }

    pub fn lamp_count(&self) -> usize {
        self.positions.len()
    }

    /// Positions for drawing: the lamp box (not the canvas) scaled so its
    /// long side runs 0 → 1, the short side 0 → `short/long`, y down.
    /// Returns `(positions, [width, height])` in those units.
    pub fn drawing_positions(&self) -> (Vec<[f32; 2]>, [f32; 2]) {
        let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
        let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for [x, y] in &self.positions {
            min_x = min_x.min(*x);
            min_y = min_y.min(*y);
            max_x = max_x.max(*x);
            max_y = max_y.max(*y);
        }
        let long = (max_x - min_x).max(max_y - min_y);
        let scale = if long > f32::EPSILON { 1.0 / long } else { 1.0 };
        let positions = self
            .positions
            .iter()
            .map(|[x, y]| [(x - min_x) * scale, (y - min_y) * scale])
            .collect();
        (
            positions,
            [(max_x - min_x) * scale, (max_y - min_y) * scale],
        )
    }

    /// Typical lamp spacing in drawing units: the median nearest-neighbour
    /// distance. O(n²) is fine on the host at swatch sizes; the page sizes
    /// its dots from it.
    pub fn drawing_pitch(positions: &[[f32; 2]]) -> f32 {
        if positions.len() < 2 {
            return 1.0;
        }
        let mut nearest: Vec<f32> = positions
            .iter()
            .enumerate()
            .map(|(i, a)| {
                positions
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, b)| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt())
                    .filter(|d| *d > 1e-6)
                    .fold(f32::INFINITY, f32::min)
            })
            .filter(|d| d.is_finite())
            .collect();
        if nearest.is_empty() {
            return 1.0;
        }
        nearest.sort_by(|a, b| a.total_cmp(b));
        nearest[nearest.len() / 2]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_positions_fit_the_long_side() {
        let swatch = Swatch {
            name: "t".into(),
            map_json: String::new(),
            positions: vec![[10.0, 5.0], [30.0, 5.0], [50.0, 15.0]],
        };
        let (positions, size) = swatch.drawing_positions();
        assert_eq!(size, [1.0, 0.25]);
        assert_eq!(positions[0], [0.0, 0.0]);
        assert_eq!(positions[2], [1.0, 0.25]);
        assert!((Swatch::drawing_pitch(&positions) - 0.5).abs() < 1e-6);
    }
}
