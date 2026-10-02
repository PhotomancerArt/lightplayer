/// The copy of a [`crate::ActionConsequence::Lasting`] action: the words that
/// say what is lost.
///
/// Every renderer reads it the same way: the armed button's `title` is the
/// `message`, and its armed label is "Confirm ⟨`confirm_label`⟩". The app
/// agent's card for the action speaks in the same words.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActionConfirmation {
    /// What is being asked, as a question ("Forget this device?").
    pub title: String,
    /// What is lost, in plain words.
    pub message: String,
    /// The verb the armed button confirms ("forget", "Delete").
    pub confirm_label: String,
}

impl ActionConfirmation {
    /// Create the copy for a lasting action.
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
