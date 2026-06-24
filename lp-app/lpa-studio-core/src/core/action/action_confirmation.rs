#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionConfirmation {
    pub title: String,
    pub message: String,
    pub confirm_label: String,
}

impl ActionConfirmation {
    pub fn new(
        title: impl Into<String>,
        message: impl Into<String>,
        confirm_label: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            message: message.into(),
            confirm_label: confirm_label.into(),
        }
    }
}
