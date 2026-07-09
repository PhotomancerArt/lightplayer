//! Configuration model + URL parsing for the preview lab (PoC A).
//!
//! The lab is driven either from its in-page controls or from URL parameters
//! (`#/preview-lab?cards=10&workers=2&fps=15&size=128&project=basic&autostart=1`),
//! which is how automated measurement sweeps select configurations.

/// Example project selectable per lab run. All examples publish their visual
/// product on the `visual.out` bus channel.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LabProject {
    #[default]
    Basic,
    Fluid,
    Events,
    FyeahSign,
}

impl LabProject {
    pub const ALL: [Self; 4] = [Self::Basic, Self::Fluid, Self::Events, Self::FyeahSign];

    pub fn key(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Fluid => "fluid",
            Self::Events => "events",
            Self::FyeahSign => "fyeah-sign",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.key() == value)
    }
}

/// One lab run's configuration.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct LabConfig {
    /// Number of preview cards (one browser runtime each).
    pub cards: u32,
    /// Number of Web Workers the runtimes are distributed across.
    pub workers: u32,
    /// Target frames per second per card.
    pub fps: u32,
    /// Square texture edge in pixels.
    pub size: u32,
    pub project: LabProject,
    /// Start the run as soon as the page mounts (automation).
    pub autostart: bool,
}

impl Default for LabConfig {
    fn default() -> Self {
        Self {
            cards: 5,
            workers: 2,
            fps: 15,
            size: 128,
            project: LabProject::Basic,
            autostart: false,
        }
    }
}

impl LabConfig {
    pub const CARD_CHOICES: [u32; 5] = [1, 5, 10, 20, 40];
    pub const WORKER_CHOICES: [u32; 4] = [1, 2, 3, 4];
    pub const FPS_CHOICES: [u32; 3] = [10, 15, 20];
    pub const SIZE_CHOICES: [u32; 3] = [64, 96, 128];

    /// Target frame period in milliseconds.
    pub fn period_ms(&self) -> f64 {
        1_000.0 / self.fps.max(1) as f64
    }

    /// Parse the lab hash route, e.g.
    /// `#/preview-lab?cards=10&workers=2&fps=15&size=128&project=basic&autostart=1`.
    ///
    /// Returns `None` when the hash is not the lab route; unknown or invalid
    /// parameters fall back to defaults.
    pub fn parse_hash(hash: &str) -> Option<Self> {
        let route = hash.strip_prefix("#/preview-lab")?;
        let query = route.strip_prefix('?').unwrap_or("");
        let mut config = Self::default();
        for (key, value) in query.split('&').filter_map(|part| part.split_once('=')) {
            match key {
                "cards" => {
                    if let Ok(cards) = value.parse::<u32>() {
                        config.cards = cards.clamp(1, 100);
                    }
                }
                "workers" => {
                    if let Ok(workers) = value.parse::<u32>() {
                        config.workers = workers.clamp(1, 8);
                    }
                }
                "fps" => {
                    if let Ok(fps) = value.parse::<u32>() {
                        config.fps = fps.clamp(1, 60);
                    }
                }
                "size" => {
                    if let Ok(size) = value.parse::<u32>() {
                        config.size = size.clamp(8, 512);
                    }
                }
                "project" => {
                    if let Some(project) = LabProject::parse(value) {
                        config.project = project;
                    }
                }
                "autostart" => config.autostart = value == "1" || value == "true",
                _ => {}
            }
        }
        Some(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_lab_hash() {
        let config = LabConfig::parse_hash(
            "#/preview-lab?cards=40&workers=4&fps=20&size=64&project=fyeah-sign&autostart=1",
        )
        .expect("lab route");

        assert_eq!(
            config,
            LabConfig {
                cards: 40,
                workers: 4,
                fps: 20,
                size: 64,
                project: LabProject::FyeahSign,
                autostart: true,
            }
        );
    }

    #[test]
    fn bare_route_uses_defaults() {
        let config = LabConfig::parse_hash("#/preview-lab").expect("lab route");
        assert_eq!(config, LabConfig::default());
    }

    #[test]
    fn non_lab_routes_do_not_parse() {
        assert!(LabConfig::parse_hash("#/stories/base/icon/overview").is_none());
        assert!(LabConfig::parse_hash("").is_none());
    }

    #[test]
    fn invalid_values_fall_back_to_defaults() {
        let config =
            LabConfig::parse_hash("#/preview-lab?cards=zero&fps=999&project=nope").expect("route");
        assert_eq!(config.cards, LabConfig::default().cards);
        assert_eq!(config.fps, 60); // clamped
        assert_eq!(config.project, LabProject::Basic);
    }

    #[test]
    fn period_follows_fps() {
        let config = LabConfig {
            fps: 20,
            ..LabConfig::default()
        };
        assert!((config.period_ms() - 50.0).abs() < 1e-9);
    }
}
