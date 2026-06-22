#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UxStatusKind {
    Neutral,
    Working,
    Good,
    Warning,
    Error,
}
