/// Metadata for one generated Studio story.
///
/// Story authors do not construct this by hand. Story files declare
/// `#[story(label = "...", description = "...")]` functions, and
/// `lpa-studio-web/build.rs` infers the family/category/component/story fields
/// from the file path plus function name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoryDescriptor {
    pub id: &'static str,
    pub family: &'static str,
    pub category: Option<&'static str>,
    pub component: &'static str,
    pub story: &'static str,
    pub label: &'static str,
    pub description: &'static str,
}

impl StoryDescriptor {
    pub const fn new(
        id: &'static str,
        family: &'static str,
        category: Option<&'static str>,
        component: &'static str,
        story: &'static str,
        label: &'static str,
        description: &'static str,
    ) -> Self {
        Self {
            id,
            family,
            category,
            component,
            story,
            label,
            description,
        }
    }

    pub fn family_label(self) -> &'static str {
        match self.family {
            "base" => "Base",
            "core" => "Core",
            "studio" => "Studio",
            "exploration" => "Exploration",
            _ => self.family,
        }
    }
}
