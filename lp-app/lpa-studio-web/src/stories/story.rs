/// Metadata for one generated Studio story.
///
/// Story authors do not construct this by hand. Story files declare
/// `#[story]` functions, and `lpa-studio-web/build.rs` infers the
/// family/category/component/story fields from the file path plus function name.
/// Labels are derived from function names unless a story provides an explicit
/// `label = "..."` override.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoryDescriptor {
    pub id: &'static str,
    pub source_path: &'static str,
    pub family: &'static str,
    pub category: Option<&'static str>,
    pub component: &'static str,
    pub story: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    /// `#[story(screenshot)]`: this capture is a published image (README
    /// hero, docs figure), not a design-record baseline. It renders bare —
    /// no story frame, size label, or checkerboard — and is captured at
    /// `lg` only.
    pub screenshot: bool,
}

impl StoryDescriptor {
    #[allow(
        clippy::too_many_arguments,
        reason = "constructed only by the generated registry, never by hand"
    )]
    pub const fn new(
        id: &'static str,
        source_path: &'static str,
        family: &'static str,
        category: Option<&'static str>,
        component: &'static str,
        story: &'static str,
        label: &'static str,
        description: &'static str,
        screenshot: bool,
    ) -> Self {
        Self {
            id,
            source_path,
            family,
            category,
            component,
            story,
            label,
            description,
            screenshot,
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
