//! What the editor says back after an action — rustc-style for refusals:
//! what happened, why, and what to do instead (D20). Never a silent no.

#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub kind: NoticeKind,
    /// What happened, in one line.
    pub text: String,
    /// Why — the reason a refusal is a refusal.
    pub why: Option<String>,
    /// What to do instead.
    pub help: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeKind {
    Info,
    Refused,
}

impl Notice {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            kind: NoticeKind::Info,
            text: text.into(),
            why: None,
            help: None,
        }
    }

    pub fn refused(
        text: impl Into<String>,
        why: impl Into<String>,
        help: impl Into<String>,
    ) -> Self {
        Self {
            kind: NoticeKind::Refused,
            text: text.into(),
            why: Some(why.into()),
            help: Some(help.into()),
        }
    }
}
