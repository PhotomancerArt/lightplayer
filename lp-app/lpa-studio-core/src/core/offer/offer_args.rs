//! [`OfferArgs`]: the values one press of an offer carries, by parameter
//! name.

use std::collections::BTreeMap;

/// The values a press carries, keyed by [`crate::OfferParam::name`].
///
/// Values travel as text, the way a field, a picker and the agent all hand
/// them over; the typed getters read them back. What a press may carry is
/// checked against the offer's parameters by [`crate::UiOffer::press`]
/// before any binder sees it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct OfferArgs(BTreeMap<String, String>);

impl OfferArgs {
    /// No values.
    pub fn new() -> Self {
        Self::default()
    }

    /// These args plus `name = value`.
    pub fn with(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.insert(name, value);
        self
    }

    /// Set `name` to `value`, replacing any earlier value.
    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.0.insert(name.into(), value.into());
    }

    /// The raw value of `name`, if the press carried one.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    /// The picked option of a choice parameter.
    pub fn choice(&self, name: &str) -> Option<&str> {
        self.get(name)
    }

    /// A text parameter, trimmed; `None` when it is absent or blank, which
    /// is what an optional field left empty means.
    pub fn text(&self, name: &str) -> Option<&str> {
        self.get(name)
            .map(str::trim)
            .filter(|text| !text.is_empty())
    }

    /// A toggle parameter; `None` when absent or not `true`/`false`.
    pub fn toggle(&self, name: &str) -> Option<bool> {
        match self.get(name)? {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        }
    }

    /// Whether the press carried nothing.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Every `(name, value)`, by name.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_getters_read_what_the_press_carried() {
        let args = OfferArgs::new()
            .with("board", "xiao")
            .with("name", "  Desk  ")
            .with("blank", "   ")
            .with("enabled", "true")
            .with("odd", "yes");

        assert_eq!(args.choice("board"), Some("xiao"));
        assert_eq!(args.text("name"), Some("Desk"), "trimmed");
        assert_eq!(args.text("blank"), None, "a blank field is no value");
        assert_eq!(args.get("blank"), Some("   "), "but the raw value stays");
        assert_eq!(args.toggle("enabled"), Some(true));
        assert_eq!(args.toggle("odd"), None);
        assert_eq!(args.get("missing"), None);
        assert!(!args.is_empty());
        assert_eq!(
            args.iter().map(|(name, _)| name).collect::<Vec<_>>(),
            ["blank", "board", "enabled", "name", "odd"]
        );
        assert!(OfferArgs::new().is_empty());
    }
}
