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

use crate::Camera;

/// The fit the camera currently carries: the viewport it was computed
/// against and the camera it produced. `None` until a fit has run.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct FitReconcile(Option<([f32; 2], Camera)>);

impl FitReconcile {
    /// Must the fit re-run for `viewport`? True when the measurement
    /// moved while `camera` still equals the fitted value — the camera is
    /// untouched, so it should track the settled layout, not the first
    /// measurement. False once the user has panned or zoomed (the camera
    /// is theirs) and false while no fit has run (that is the armed
    /// `fit_pending` path's job).
    #[must_use]
    pub fn stale(&self, viewport: [f32; 2], camera: &Camera) -> bool {
        matches!(self.0, Some((fitted_vp, fitted_cam))
            if fitted_vp != viewport && fitted_cam == *camera)
    }

    /// Record a completed reconciliation of `camera` against `viewport`.
    pub fn record(&mut self, viewport: [f32; 2], camera: Camera) {
        self.0 = Some((viewport, camera));
    }

    /// The `data-fit-viewport` value the story-capture ready gate checks:
    /// the reconciled size (whole pixels), or `""` while no measurement
    /// has been reconciled. The gate refuses to photograph a visible
    /// canvas whose real box disagrees, so a fit that consumed a stale
    /// measurement fails loudly instead of flapping baselines.
    #[must_use]
    pub fn guard_attr(&self) -> String {
        self.0
            .map(|([width, height], _)| {
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

    #[test]
    fn never_stale_before_the_first_fit() {
        let fit = FitReconcile::default();
        assert!(!fit.stale(VP1, &cam(1.0)));
        assert_eq!(fit.guard_attr(), "");
    }

    #[test]
    fn viewport_change_with_untouched_camera_is_stale() {
        let mut fit = FitReconcile::default();
        fit.record(VP1, cam(2.0));
        assert!(!fit.stale(VP1, &cam(2.0)), "same viewport: settled");
        assert!(
            fit.stale(VP2, &cam(2.0)),
            "moved viewport, untouched camera"
        );
    }

    #[test]
    fn a_touched_camera_is_never_reconciled_again() {
        let mut fit = FitReconcile::default();
        fit.record(VP1, cam(2.0));
        // The user zoomed: the camera no longer equals the fitted value.
        assert!(!fit.stale(VP2, &cam(3.0)));
    }

    #[test]
    fn guard_attr_names_the_reconciled_viewport() {
        let mut fit = FitReconcile::default();
        fit.record([517.5, 399.6], cam(1.0));
        assert_eq!(fit.guard_attr(), "518x400");
    }
}
