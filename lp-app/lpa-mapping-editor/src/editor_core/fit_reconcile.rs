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
//!
//! The two halves are NOT interchangeable, which is why [`staleness`]
//! names which one moved. A viewport change must always re-fit (that is
//! the determinism above). A content change is a weaker signal: the host
//! decides, and the workbench declines to re-frame content the user can
//! already see — diving into a fixture that is on screen at a workable
//! size should not yank the camera (`Camera::frames_well`).
//!
//! [`staleness`]: FitReconcile::staleness

use crate::Camera;

/// Why a fit is out of date — the two halves reconcile differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FitStale {
    /// The fit still matches its measurements, or the camera is the
    /// user's and reconciliation has stopped.
    Settled,
    /// The VIEWPORT moved. The fit must track it unconditionally: a fit
    /// that keeps whatever size the container had at first measurement is
    /// the baseline churner this module exists to kill.
    Viewport,
    /// Only the CONTENT moved. Hosts may decline to re-frame content the
    /// user can already see (`Camera::frames_well`) — an arriving body
    /// still gets framed, a fixture already on screen is left alone.
    Bounds,
}

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
    /// Which measurement moved out from under the fit, if either. A
    /// measurement only counts while `camera` still equals the fitted
    /// value: once the user has panned or zoomed the camera is theirs and
    /// reconciliation stops ([`FitStale::Settled`]), as it does while no
    /// fit has run at all (that is the armed `fit_pending` path's job).
    ///
    /// The viewport half outranks the content half — a fit that consumed
    /// an unsettled container size must be redone whatever the content is
    /// doing.
    #[must_use]
    pub fn staleness(
        &self,
        viewport: [f32; 2],
        camera: &Camera,
        bounds: Option<[f32; 4]>,
    ) -> FitStale {
        let Some(fitted) = self.0 else {
            return FitStale::Settled;
        };
        if fitted.camera != *camera {
            return FitStale::Settled;
        }
        if fitted.viewport != viewport {
            return FitStale::Viewport;
        }
        if bounds_close(fitted.bounds, bounds) {
            return FitStale::Settled;
        }
        FitStale::Bounds
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

    /// The content bounds the last fit consumed. Hosts substitute this for
    /// the live bounds once the USER starts editing content: bounds
    /// reconciliation exists to settle ASYNC ARRIVALS (bodies, the
    /// arrangement document), and a user's own drag moving the bounds must
    /// never re-fit the view under the gesture (G1 round 2).
    #[must_use]
    pub fn fitted_bounds(&self) -> Option<[f32; 4]> {
        self.0.and_then(|fitted| fitted.bounds)
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
        assert_eq!(fit.staleness(VP1, &cam(1.0), B1), FitStale::Settled);
        assert_eq!(fit.guard_attr(), "");
    }

    #[test]
    fn viewport_change_with_untouched_camera_is_stale() {
        let mut fit = FitReconcile::default();
        fit.record(VP1, cam(2.0), B1);
        assert_eq!(
            fit.staleness(VP1, &cam(2.0), B1),
            FitStale::Settled,
            "same measurements: settled"
        );
        assert_eq!(
            fit.staleness(VP2, &cam(2.0), B1),
            FitStale::Viewport,
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
        assert_eq!(
            fit.staleness(VP1, &cam(2.0), B2),
            FitStale::Bounds,
            "content moved under the fit"
        );
        assert_eq!(
            fit.staleness(VP1, &cam(2.0), Some([0.5, 0.0, 100.0, 100.4])),
            FitStale::Settled,
            "sub-percent creep is not churn"
        );
        assert_eq!(
            fit.staleness(VP1, &cam(2.0), None),
            FitStale::Bounds,
            "content vanished — reconcile the empty canvas too"
        );
    }

    /// A viewport change outranks a simultaneous content change: the host
    /// may decline a content-only re-frame, never a viewport one.
    #[test]
    fn a_moved_viewport_outranks_moved_content() {
        let mut fit = FitReconcile::default();
        fit.record(VP1, cam(2.0), B1);
        assert_eq!(fit.staleness(VP2, &cam(2.0), B2), FitStale::Viewport);
    }

    #[test]
    fn a_touched_camera_is_never_reconciled_again() {
        let mut fit = FitReconcile::default();
        fit.record(VP1, cam(2.0), B1);
        // The user zoomed: the camera no longer equals the fitted value.
        assert_eq!(fit.staleness(VP2, &cam(3.0), B2), FitStale::Settled);
    }

    #[test]
    fn guard_attr_names_the_reconciled_viewport() {
        let mut fit = FitReconcile::default();
        fit.record([517.5, 399.6], cam(1.0), B1);
        assert_eq!(fit.guard_attr(), "518x400");
    }
}
