//! Output overrides: what the selection does to an output's own pixels,
//! after the patch has placed its control products on them (D12, D13,
//! D55) — the product's `highlight`, its fault breathe and test pattern,
//! which paint only the output's buffer.
//!
//! A selected output breathes across every port, a selected port across
//! its whole range, and a selected pixel (or a run of them) on its own —
//! patched or not, so the user can always reach the hardware (D13, D47).
//! While nothing inside a fixture is selected too, every other pixel of
//! every output dims to [`DIM`], so the selection is always shown (D16).
//!
//! None of it reaches back: a fixture's control product, and so every lamp
//! on the canvas, is the same whatever is selected on the output side.

use crate::breath::{DIM, breath};
use crate::fixture_overrides::FixtureOverrides;
use crate::node::NodeId;
use crate::project::Project;
use crate::rgb::Rgb;
use crate::target::Target;

/// The override rules for one frame, worked out once from the selection.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OutputOverrides {
    /// What breathes: (output, port key, pixel). No port means every port
    /// of the output; no pixel means the whole port.
    picks: Vec<(NodeId, Option<String>, Option<u32>)>,
    /// Everything not picked dims.
    dim_rest: bool,
}

impl OutputOverrides {
    /// The rules a selection makes: only its output-side targets count.
    pub fn of(project: &Project, selection: &[Target], fixture: &FixtureOverrides) -> Self {
        let mut picks = Vec::new();
        for t in selection {
            match t {
                Target::Node(id) if project.output(id).is_some() => {
                    picks.push((id.clone(), None, None))
                }
                Target::Port { output, key } => {
                    picks.push((output.clone(), Some(key.clone()), None))
                }
                Target::Pixel { output, key, index } => {
                    picks.push((output.clone(), Some(key.clone()), Some(*index)))
                }
                _ => {}
            }
        }
        let dim_rest = !picks.is_empty() && *fixture == FixtureOverrides::None;
        OutputOverrides { picks, dim_rest }
    }

    /// Paint one port's pixels.
    pub fn apply(&self, output: &NodeId, key: &str, pixels: &mut [Rgb], time: f64) {
        if self.picks.is_empty() {
            return;
        }
        let mine: Vec<Option<u32>> = self
            .picks
            .iter()
            .filter(|(o, k, _)| o == output && k.as_deref().is_none_or(|k| k == key))
            .map(|(_, _, px)| *px)
            .collect();
        let whole = mine.iter().any(Option::is_none);
        let breath = breath(time);
        for (i, px) in pixels.iter_mut().enumerate() {
            if whole || mine.contains(&Some(i as u32)) {
                *px = breath;
            } else if self.dim_rest {
                *px = px.scaled(DIM);
            }
        }
    }
}
