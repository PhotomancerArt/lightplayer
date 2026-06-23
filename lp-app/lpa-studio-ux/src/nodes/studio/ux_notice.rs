#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UxNoticeLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UxNotice {
    pub level: UxNoticeLevel,
    pub message: String,
}

impl UxNotice {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            level: UxNoticeLevel::Info,
            message: message.into(),
        }
    }
}
