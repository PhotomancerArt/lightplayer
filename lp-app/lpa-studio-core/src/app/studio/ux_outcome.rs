use crate::UxNotice;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UxOutcome {
    pub notices: Vec<UxNotice>,
}

impl UxOutcome {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_notice(mut self, notice: UxNotice) -> Self {
        self.notices.push(notice);
        self
    }
}
