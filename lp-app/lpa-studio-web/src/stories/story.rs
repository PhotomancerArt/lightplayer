/// Metadata for one Studio component story.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoryDescriptor {
    pub id: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

impl StoryDescriptor {
    pub const fn new(
        id: &'static str,
        group: &'static str,
        label: &'static str,
        description: &'static str,
    ) -> Self {
        Self {
            id,
            group,
            label,
            description,
        }
    }
}
