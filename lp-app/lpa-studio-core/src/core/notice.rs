#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoticeLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiNotice {
    pub level: NoticeLevel,
    pub message: String,
}

impl UiNotice {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            level: NoticeLevel::Info,
            message: message.into(),
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            level: NoticeLevel::Warning,
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
