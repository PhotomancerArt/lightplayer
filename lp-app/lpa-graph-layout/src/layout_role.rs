//! What a node is to the layout. The layout places nodes by role, not by
//! reading the graph's shape: LightPlayer can tell what a node is from its
//! kind, so it doesn't have to be discovered.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum LayoutRole {
    /// A clock, a power button, a button, a radio: no dataflow in, so they
    /// sit in a row above the pipeline.
    Control,
    /// A shader, a texture, a playlist: they make pictures that fixtures
    /// sample, so they are the pipeline's first column.
    Source,
    /// The pipeline's second column.
    Fixture,
    /// The third column, each level with what feeds it.
    Output,
    /// A frame around its children, laid out by the same rules inside.
    Module,
    /// Anything else: a column of its own at the right.
    Other,
}
