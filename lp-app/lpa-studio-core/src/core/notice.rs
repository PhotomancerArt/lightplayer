#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiNoticeLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiNotice {
    pub level: UiNoticeLevel,
    pub message: String,
}

impl UiNotice {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            level: UiNoticeLevel::Info,
            message: message.into(),
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            level: UiNoticeLevel::Warning,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UiNotices {
    pub notices: Vec<UiNotice>,
}

impl UiNotices {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_notice(mut self, notice: UiNotice) -> Self {
        self.notices.push(notice);
        self
    }
}
