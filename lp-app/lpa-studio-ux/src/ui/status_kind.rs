#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiStatusKind {
    Neutral,
    Working,
    Good,
    Warning,
    Error,
}
