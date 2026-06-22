use lpa_link::LinkProviderKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderChoice {
    pub id: LinkProviderKind,
    pub label: String,
    pub summary: String,
}

impl ProviderChoice {
    pub fn browser_worker() -> Self {
        Self {
            id: LinkProviderKind::BrowserWorker,
            label: "Simulator".to_string(),
            summary: "Run LightPlayer locally in a browser worker.".to_string(),
        }
    }
}
