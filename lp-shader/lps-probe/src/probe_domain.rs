//! [`ProbeDomain`]: where a probe expression is evaluated.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

use crate::experiment_spec::LedPoint;

/// Evaluation domain for one probe. Position coordinates are normalized
/// [0,1]² (scaled to pixel space by `ExperimentSpec::size` at call time).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeDomain {
    /// One point.
    Point { at: [f32; 2] },
    /// An explicit list of points.
    Points { at: Vec<[f32; 2]> },
    /// `nx × ny` cell centers of `rect` (`[x0, y0, x1, y1]`, default the
    /// unit square): site `(i, j)` is at `(x0 + (i+0.5)·w/nx, y0 + (j+0.5)·h/ny)`,
    /// row-major (`j` outer, `i` inner).
    Grid {
        nx: u16,
        ny: u16,
        #[serde(default)]
        rect: Option<[f32; 4]>,
    },
    /// `n` evenly spaced points from `from` to `to` inclusive.
    Line {
        from: [f32; 2],
        to: [f32; 2],
        n: u16,
    },
    /// LED points from `ExperimentSpec::led_points`; `None` = all of them.
    Leds {
        #[serde(default)]
        indices: Option<Vec<u32>>,
    },
    /// Scalar parameter sweep: the wrapper takes `float <var>` instead of
    /// `vec2 pos`; `n` evenly spaced values from `from` to `to` inclusive.
    Sweep {
        var: String,
        from: f32,
        to: f32,
        n: u16,
    },
}

/// One evaluation site: a normalized position, or a sweep-variable value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProbeSite {
    Pos([f32; 2]),
    Var(f32),
}

impl ProbeDomain {
    /// Enumerate the sites of this domain, in the documented deterministic
    /// order. Errors are actionable messages (invalid counts or indices).
    pub fn sites(&self, led_points: &[LedPoint]) -> Result<Vec<ProbeSite>, String> {
        match self {
            ProbeDomain::Point { at } => Ok(alloc::vec![ProbeSite::Pos(*at)]),
            ProbeDomain::Points { at } => Ok(at.iter().map(|p| ProbeSite::Pos(*p)).collect()),
            ProbeDomain::Grid { nx, ny, rect } => {
                if *nx == 0 || *ny == 0 {
                    return Err(format!(
                        "grid domain needs nx > 0 and ny > 0 (got {nx}×{ny})"
                    ));
                }
                let rect = rect.unwrap_or([0.0, 0.0, 1.0, 1.0]);
                Ok(grid_sites(*nx, *ny, rect)
                    .into_iter()
                    .map(ProbeSite::Pos)
                    .collect())
            }
            ProbeDomain::Line { from, to, n } => {
                if *n == 0 {
                    return Err(String::from("line domain needs n > 0"));
                }
                Ok(lerp_steps(*n)
                    .map(|t| {
                        ProbeSite::Pos([
                            from[0] + (to[0] - from[0]) * t,
                            from[1] + (to[1] - from[1]) * t,
                        ])
                    })
                    .collect())
            }
            ProbeDomain::Leds { indices } => match indices {
                None => Ok(led_points.iter().map(|p| ProbeSite::Pos(p.at)).collect()),
                Some(idx) => idx
                    .iter()
                    .map(|&i| {
                        led_points
                            .get(i as usize)
                            .map(|p| ProbeSite::Pos(p.at))
                            .ok_or_else(|| {
                                format!(
                                    "led index {i} out of range ({} led points supplied)",
                                    led_points.len()
                                )
                            })
                    })
                    .collect(),
            },
            ProbeDomain::Sweep { from, to, n, .. } => {
                if *n == 0 {
                    return Err(String::from("sweep domain needs n > 0"));
                }
                Ok(lerp_steps(*n)
                    .map(|t| ProbeSite::Var(from + (to - from) * t))
                    .collect())
            }
        }
    }
}

/// Cell centers of `rect` on an `nx × ny` grid, row-major (`y` outer).
/// Shared by probe grids, health, and diff.
pub fn grid_sites(nx: u16, ny: u16, rect: [f32; 4]) -> Vec<[f32; 2]> {
    let [x0, y0, x1, y1] = rect;
    let (w, h) = (x1 - x0, y1 - y0);
    let mut out = Vec::with_capacity(nx as usize * ny as usize);
    for j in 0..ny {
        for i in 0..nx {
            out.push([
                x0 + (i as f32 + 0.5) * w / nx as f32,
                y0 + (j as f32 + 0.5) * h / ny as f32,
            ]);
        }
    }
    out
}

/// Interpolation parameters for `n` inclusive steps: `0, 1/(n-1), …, 1`
/// (`n == 1` yields just `0`).
fn lerp_steps(n: u16) -> impl Iterator<Item = f32> {
    (0..n).map(move |i| {
        if n <= 1 {
            0.0
        } else {
            i as f32 / (n - 1) as f32
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_sites_are_cell_centers() {
        let sites = grid_sites(2, 2, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(
            sites,
            alloc::vec![[0.25, 0.25], [0.75, 0.25], [0.25, 0.75], [0.75, 0.75]]
        );
    }

    #[test]
    fn line_is_inclusive() {
        let d = ProbeDomain::Line {
            from: [0.0, 0.0],
            to: [1.0, 0.0],
            n: 3,
        };
        let sites = d.sites(&[]).expect("sites");
        assert_eq!(
            sites,
            alloc::vec![
                ProbeSite::Pos([0.0, 0.0]),
                ProbeSite::Pos([0.5, 0.0]),
                ProbeSite::Pos([1.0, 0.0])
            ]
        );
    }

    #[test]
    fn led_indices_are_validated() {
        let leds = alloc::vec![LedPoint {
            label: alloc::string::String::from("a"),
            channel: 0,
            at: [0.5, 0.5],
        }];
        let ok = ProbeDomain::Leds {
            indices: Some(alloc::vec![0]),
        };
        assert_eq!(
            ok.sites(&leds).expect("ok"),
            alloc::vec![ProbeSite::Pos([0.5, 0.5])]
        );
        let bad = ProbeDomain::Leds {
            indices: Some(alloc::vec![3]),
        };
        let err = bad.sites(&leds).expect_err("out of range");
        assert!(err.contains("led index 3"), "{err}");
    }
}
