/// Confirmation metadata for risky available actions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionConfirmation {
    pub title: &'static str,
    pub message: &'static str,
    pub confirm_label: &'static str,
    pub destructive: bool,
}

impl ActionConfirmation {
    pub fn new(
        title: &'static str,
        message: &'static str,
        confirm_label: &'static str,
        destructive: bool,
    ) -> Self {
        Self {
            title,
            message,
            confirm_label,
            destructive,
        }
    }
}
