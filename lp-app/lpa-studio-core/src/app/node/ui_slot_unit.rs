//! Renderable unit metadata for Studio node surfaces.

/// Studio-facing physical or semantic unit labels.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiSlotUnit {
    /// Compact unit label used near values.
    pub short: String,
    /// Verbose unit label used in detail popups.
    pub long: String,
}

impl UiSlotUnit {
    /// Create a unit label pair.
    pub fn new(short: impl Into<String>, long: impl Into<String>) -> Self {
        Self {
            short: short.into(),
            long: long.into(),
        }
    }

    /// Seconds.
    pub fn seconds() -> Self {
        Self::new("s", "seconds")
    }

    /// Milliseconds.
    pub fn milliseconds() -> Self {
        Self::new("ms", "milliseconds")
    }

    /// Hertz.
    pub fn hertz() -> Self {
        Self::new("Hz", "hertz")
    }

    /// Radians.
    pub fn radians() -> Self {
        Self::new("rad", "radians")
    }

    /// Degrees.
    pub fn degrees() -> Self {
        Self::new("deg", "degrees")
    }

    /// Percent.
    pub fn percent() -> Self {
        Self::new("%", "percent")
    }

    /// Recognize common existing unit labels while older DTOs are migrated.
    pub fn from_known_label(label: &str) -> Option<Self> {
        match label.trim().to_ascii_lowercase().as_str() {
            "s" | "sec" | "secs" | "second" | "seconds" => Some(Self::seconds()),
            "ms" | "millisecond" | "milliseconds" => Some(Self::milliseconds()),
            "hz" | "hertz" => Some(Self::hertz()),
            "rad" | "radian" | "radians" => Some(Self::radians()),
            "deg" | "degree" | "degrees" => Some(Self::degrees()),
            "%" | "percent" | "percentage" => Some(Self::percent()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::UiSlotUnit;

    #[test]
    fn recognizes_short_and_long_labels() {
        assert_eq!(
            UiSlotUnit::from_known_label("s"),
            Some(UiSlotUnit::seconds())
        );
        assert_eq!(
            UiSlotUnit::from_known_label("seconds"),
            Some(UiSlotUnit::seconds())
        );
        assert_eq!(
            UiSlotUnit::from_known_label("Hz"),
            Some(UiSlotUnit::hertz())
        );
    }
}
