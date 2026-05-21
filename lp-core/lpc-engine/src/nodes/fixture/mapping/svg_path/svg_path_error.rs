use alloc::format;
use alloc::string::String;

#[derive(Debug, Clone, PartialEq)]
pub enum SvgPathError {
    DuplicatePathIndex(u32),
    EmptyPath { path_index: u32 },
    InvalidAttribute { name: &'static str },
    InvalidNumber(String),
    InvalidPathLabel(String),
    InvalidPolyline,
    InvalidViewBox,
    MissingPathLikeElement { path_index: u32 },
    MultiplePathLikeElements { path_index: u32 },
    MultipleTextElements,
    NestedGroup,
    NoMappingGroups,
    UngroupedMappingText(String),
    UnsupportedCommand(char),
    ZeroCount { path_index: u32 },
}

impl core::fmt::Display for SvgPathError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DuplicatePathIndex(index) => write!(f, "duplicate svg mapping path:{index}"),
            Self::EmptyPath { path_index } => write!(f, "svg mapping path:{path_index} is empty"),
            Self::InvalidAttribute { name } => write!(f, "invalid or missing SVG attribute {name}"),
            Self::InvalidNumber(value) => write!(f, "invalid SVG number {value:?}"),
            Self::InvalidPathLabel(label) => write!(f, "invalid SVG mapping text {label:?}"),
            Self::InvalidPolyline => write!(f, "invalid SVG polyline points"),
            Self::InvalidViewBox => write!(f, "invalid SVG viewBox"),
            Self::MissingPathLikeElement { path_index } => {
                write!(f, "svg mapping path:{path_index} has no path or polyline")
            }
            Self::MultiplePathLikeElements { path_index } => {
                write!(
                    f,
                    "svg mapping path:{path_index} has multiple paths/polylines"
                )
            }
            Self::MultipleTextElements => write!(f, "svg mapping group has multiple text elements"),
            Self::NestedGroup => write!(f, "nested SVG mapping groups are not supported"),
            Self::NoMappingGroups => write!(f, "SVG contains no mapping groups"),
            Self::UngroupedMappingText(text) => {
                write!(f, "SVG mapping text {text:?} is not inside a valid group")
            }
            Self::UnsupportedCommand(command) => {
                write!(f, "unsupported SVG path command {command:?}")
            }
            Self::ZeroCount { path_index } => {
                write!(f, "svg mapping path:{path_index} has count:0")
            }
        }
    }
}

impl core::error::Error for SvgPathError {}

pub fn invalid_number(value: &str) -> SvgPathError {
    SvgPathError::InvalidNumber(String::from(value))
}

pub fn invalid_label(value: &str) -> SvgPathError {
    SvgPathError::InvalidPathLabel(format!("{value}"))
}
