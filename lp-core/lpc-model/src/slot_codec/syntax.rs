use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

/// Maximum string chunk emitted by syntax sources.
pub(crate) const STRING_CHUNK_SIZE: usize = 1024;

/// Byte span in the source syntax input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    pub(crate) fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// Shape-agnostic syntax event.
#[derive(Clone, Debug, PartialEq)]
pub enum SyntaxEvent {
    StartObject {
        span: Option<SourceSpan>,
    },
    Prop {
        name: String,
        span: Option<SourceSpan>,
    },
    EndObject {
        span: Option<SourceSpan>,
    },
    StartArray {
        span: Option<SourceSpan>,
    },
    EndArray {
        span: Option<SourceSpan>,
    },
    StringChunk {
        text: String,
        is_last: bool,
        span: Option<SourceSpan>,
    },
    Number {
        text: String,
        span: Option<SourceSpan>,
    },
    Bool {
        value: bool,
        span: Option<SourceSpan>,
    },
    Null {
        span: Option<SourceSpan>,
    },
}

impl SyntaxEvent {
    pub(crate) fn span(&self) -> Option<SourceSpan> {
        match self {
            Self::StartObject { span }
            | Self::Prop { span, .. }
            | Self::EndObject { span }
            | Self::StartArray { span }
            | Self::EndArray { span }
            | Self::StringChunk { span, .. }
            | Self::Number { span, .. }
            | Self::Bool { span, .. }
            | Self::Null { span } => *span,
        }
    }
}

/// Pull-based source for syntax events.
pub trait SyntaxEventSource {
    fn next_event(&mut self) -> Result<Option<SyntaxEvent>, SyntaxError>;
}

/// Error returned by syntax readers and adapters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxError {
    path: String,
    span: Option<SourceSpan>,
    message: String,
}

impl SyntaxError {
    pub(crate) fn new(
        path: impl Into<String>,
        span: Option<SourceSpan>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            span,
            message: message.into(),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn span(&self) -> Option<SourceSpan> {
        self.span
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

impl core::error::Error for SyntaxError {}

pub(crate) fn split_string_events(value: &str, span: Option<SourceSpan>) -> Vec<SyntaxEvent> {
    if value.is_empty() {
        return alloc::vec![SyntaxEvent::StringChunk {
            text: String::new(),
            is_last: true,
            span,
        }];
    }

    let mut events = Vec::new();
    let mut start = 0;
    while start < value.len() {
        let mut end = (start + STRING_CHUNK_SIZE).min(value.len());
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        events.push(SyntaxEvent::StringChunk {
            text: value[start..end].into(),
            is_last: end == value.len(),
            span,
        });
        start = end;
    }
    events
}
