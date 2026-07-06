//! The closed set of subsystems a Studio console entry can originate from.

/// Where a log entry came from, as a closed, filterable set.
///
/// Origins are the console's only source-filter dimension. Free-form context
/// (module path, endpoint id, transport label) rides along as
/// [`UiLogSource::detail`](super::UiLogSource) and is display/search text
/// only, never a filter dimension.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UiLogOrigin {
    /// The Studio application core itself: action outcomes, dispatch errors,
    /// connect/sync bookkeeping.
    Studio,
    /// The device link layer: `lpa-link` providers, transports (including the
    /// browser-serial transport), and device management runs.
    Link,
    /// The LightPlayer server protocol: `lp-server` events, heartbeats, and
    /// protocol diagnostics.
    Server,
    /// The device runtime: firmware serial output (`fw-esp32`) and simulator
    /// worker logs (`fw-browser` / worker targets).
    Device,
}

impl UiLogOrigin {
    /// Every origin, in the stable order used by filter toggles and views.
    pub const ALL: [UiLogOrigin; 4] = [
        UiLogOrigin::Studio,
        UiLogOrigin::Link,
        UiLogOrigin::Server,
        UiLogOrigin::Device,
    ];

    /// Short lowercase display label.
    pub fn label(self) -> &'static str {
        match self {
            UiLogOrigin::Studio => "studio",
            UiLogOrigin::Link => "link",
            UiLogOrigin::Server => "server",
            UiLogOrigin::Device => "device",
        }
    }

    /// Position of this origin in [`UiLogOrigin::ALL`], used by the filter's
    /// per-origin toggle storage.
    pub(crate) fn index(self) -> usize {
        match self {
            UiLogOrigin::Studio => 0,
            UiLogOrigin::Link => 1,
            UiLogOrigin::Server => 2,
            UiLogOrigin::Device => 3,
        }
    }
}
