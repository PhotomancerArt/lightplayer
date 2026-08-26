//! Fit reconciliation: which viewport measurement the camera's fit
//! actually consumed.
//!
//! A fit that runs on the FIRST measurement freezes whatever size the
//! container happened to have at that instant — and the first measurement
//! races layout settling (dock widths, the mobile fold, stylesheet
//! arrival), so the same mount can land at wildly different zooms
//! run-to-run. That race is the story-baseline churner class
//! (`docs/debt/story-capture-pipeline.md`: the workbench Mapping canvas
//! oscillated between 82% and 157% zoom across two CI captures of the
//! same story). The rule here makes the fit a function of the SETTLED
//! viewport instead of measurement timing: while the camera is still
//! exactly the value the last fit produced (nobody panned or zoomed), a
//! viewport change re-runs the fit; the moment the user touches the
//! camera, it is theirs and reconciliation stops.
//!
//! The CONTENT is a measurement too (unified-selection G1 round 1): map2d
//! bodies and the arrangement document arrive asynchronously, so the fit
//! can consume early placeholder bounds and leave the real fixtures out
//! of view. The same rule extends to bounds — while the camera is still
//! the fitted value, materially moved content re-runs the fit; once the
//! user touches the camera, it is theirs.

use crate::Camera;

/// One recorded fit: what it consumed and what it produced.
#[derive(Clone, Copy, Debug, PartialEq)]
struct FittedState {
    viewport: [f32; 2],
    bounds: Option<[f32; 4]>,
    camera: Camera,
}

/// The fit the camera currently carries: the viewport + content bounds it
/// was computed against and the camera it produced. `None` until a fit
/// has run.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FitReconcile(Option<FittedState>);

/// Two content bounds close enough that re-fitting would be churn (doc
/// units; content that creeps by less than this fraction of its own size
/// stays put).
fn bounds_close(a: Option<[f32; 4]>, b: Option<[f32; 4]>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let scale = a[2].abs().max(a[3].abs()).max(1.0);
            a.iter()
                .zip(b.iter())
                .all(|(x, y)| (x - y).abs() <= scale * 0.01)
        }
        _ => false,
    }
}

impl FitReconcile {
    /// Must the fit re-run for `viewport` + `bounds`? True when either
    /// measurement moved while `camera` still equals the fitted value —
    /// the camera is untouched, so it should track the settled layout and
    /// the settled CONTENT, not first arrivals. False once the user has
    /// panned or zoomed (the camera is theirs) and false while no fit has
    /// run (that is the armed `fit_pending` path's job).
    #[must_use]
    pub fn stale(&self, viewport: [f32; 2], camera: &Camera, bounds: Option<[f32; 4]>) -> bool {
        matches!(self.0, Some(fitted)
            if fitted.camera == *camera
                && (fitted.viewport != viewport || !bounds_close(fitted.bounds, bounds)))
    }

    /// Record a completed reconciliation of `camera` against `viewport`
    /// and the content `bounds` the fit framed.
    pub fn record(&mut self, viewport: [f32; 2], camera: Camera, bounds: Option<[f32; 4]>) {
        self.0 = Some(FittedState {
            viewport,
            bounds,
            camera,
        });
    }

    /// The `data-fit-viewport` value the story-capture ready gate checks:
    /// the reconciled size (whole pixels), or `""` while no measurement
    /// has been reconciled. The gate refuses to photograph a visible
    /// canvas whose real box disagrees, so a fit that consumed a stale
    /// measurement fails loudly instead of flapping baselines.
    #[must_use]
    pub fn guard_attr(&self) -> String {
        self.0
            .map(|fitted| {
                let [width, height] = fitted.viewport;
                format!("{}x{}", width.round() as i64, height.round() as i64)
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VP1: [f32; 2] = [600.0, 400.0];
    const VP2: [f32; 2] = [900.0, 400.0];

    fn cam(scale: f32) -> Camera {
        Camera {
            x: 10.0,
            y: 20.0,
            scale,
        }
    }

    const B1: Option<[f32; 4]> = Some([0.0, 0.0, 100.0, 100.0]);
    const B2: Option<[f32; 4]> = Some([300.0, 40.0, 160.0, 60.0]);

    #[test]
    fn never_stale_before_the_first_fit() {
        let fit = FitReconcile::default();
        assert!(!fit.stale(VP1, &cam(1.0), B1));
        assert_eq!(fit.guard_attr(), "");
    }

    #[test]
    fn viewport_change_with_untouched_camera_is_stale() {
        let mut fit = FitReconcile::default();
        fit.record(VP1, cam(2.0), B1);
        assert!(!fit.stale(VP1, &cam(2.0), B1), "same measurements: settled");
        assert!(
            fit.stale(VP2, &cam(2.0), B1),
            "moved viewport, untouched camera"
        );
    }

    /// The async-content half (G1 round 1): bodies and the arrangement
    /// arrive late, so materially moved BOUNDS re-fit too — the peach
    /// must not open out of view.
    #[test]
    fn bounds_change_with_untouched_camera_is_stale() {
        let mut fit = FitReconcile::default();
        fit.record(VP1, cam(2.0), B1);
        assert!(fit.stale(VP1, &cam(2.0), B2), "content moved under the fit");
        assert!(
            !fit.stale(VP1, &cam(2.0), Some([0.5, 0.0, 100.0, 100.4])),
            "sub-percent creep is not churn"
        );
        assert!(
            fit.stale(VP1, &cam(2.0), None),
            "content vanished — reconcile the empty canvas too"
        );
    }

    #[test]
    fn a_touched_camera_is_never_reconciled_again() {
        let mut fit = FitReconcile::default();
        fit.record(VP1, cam(2.0), B1);
        // The user zoomed: the camera no longer equals the fitted value.
        assert!(!fit.stale(VP2, &cam(3.0), B2));
    }

    #[test]
    fn guard_attr_names_the_reconciled_viewport() {
        let mut fit = FitReconcile::default();
        fit.record([517.5, 399.6], cam(1.0), B1);
        assert_eq!(fit.guard_attr(), "518x400");
    }
}
