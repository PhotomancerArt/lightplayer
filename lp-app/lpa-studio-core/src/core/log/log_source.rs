//! Structured source attribution for console log entries.

use core::fmt;

use super::UiLogOrigin;

/// Structured source of a log entry: a filterable origin plus optional
/// free-form detail.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiLogSource {
    /// The filterable subsystem this entry came from.
    pub origin: UiLogOrigin,
    /// Module path, endpoint id, or transport label. Display/search text only,
    /// never a filter dimension.
    pub detail: Option<String>,
}

impl UiLogSource {
    /// A source with no detail.
    pub fn new(origin: UiLogOrigin) -> Self {
        Self {
            origin,
            detail: None,
        }
    }

    /// A source with display-only detail, such as a transport label
    /// (`browser-serial`) or a worker log target.
    pub fn with_detail(origin: UiLogOrigin, detail: impl Into<String>) -> Self {
        Self {
            origin,
            detail: Some(detail.into()),
        }
    }
}

/// Keeps bare-origin call sites terse: `UiLogDraft::new(level, origin, msg)`.
impl From<UiLogOrigin> for UiLogSource {
    fn from(origin: UiLogOrigin) -> Self {
        Self::new(origin)
    }
}

/// Renders the most specific label available: the detail when present (e.g.
/// `fw-esp32`, `browser-serial`), otherwise the origin label. Consoles that
/// want both dimensions should render `origin` and `detail` separately.
impl fmt::Display for UiLogSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.detail {
            Some(detail) => f.write_str(detail),
            None => f.write_str(self.origin.label()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_prefers_detail_over_origin_label() {
        let bare = UiLogSource::new(UiLogOrigin::Server);
        let detailed = UiLogSource::with_detail(UiLogOrigin::Device, "fw-esp32");

        assert_eq!(bare.to_string(), "server");
        assert_eq!(detailed.to_string(), "fw-esp32");
    }
}
