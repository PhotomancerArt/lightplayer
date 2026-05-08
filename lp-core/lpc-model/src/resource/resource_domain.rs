/// Which resource family a [`ResourceRef`] refers to.
///
/// [`ResourceRef`]: crate::resource::ResourceRef
#[derive(
    Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ResourceDomain {
    RuntimeBuffer,
    RenderProduct,
}
