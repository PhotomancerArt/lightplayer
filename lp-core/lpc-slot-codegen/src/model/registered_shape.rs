#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StaticRegisteredShape {
    pub(crate) type_path: String,
    pub(crate) has_default_factory: bool,
}
