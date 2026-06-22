#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedProjectChoice {
    pub project_id: String,
    pub handle_id: u32,
}

impl LoadedProjectChoice {
    pub fn new(project_id: impl Into<String>, handle_id: u32) -> Self {
        Self {
            project_id: project_id.into(),
            handle_id,
        }
    }
}
