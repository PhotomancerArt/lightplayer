//! What the palette chooser has to offer: the shipped catalog plus the
//! palettes this project already uses (M4 P4).
//!
//! Two sources, one row type. The catalog is `lpa-palettes`' checked-in
//! starter set, grouped by [`PaletteCategory`]; "This project" is derived —
//! the distinct gradients authored anywhere in the project graph, so a
//! palette someone built for one node is one click away on every other.
//!
//! Both arrive through **context** ([`PaletteCatalog`], provided by the
//! project workspace), the same injection the binding picker's channel
//! choices use: production fills it from the real view, stories fake it, and
//! the chooser itself never reaches for global state. A chooser rendered
//! with no provider at all still works — it falls back to the built-in
//! catalog and shows no project section.

use dioxus::prelude::*;
use lpa_palettes::{PaletteCategory, PaletteLicense};
use lpa_studio_core::app::project::gradient_config_value;
use lpa_studio_core::{
    UiConfigSlot, UiConfigSlotBody, UiNodeChild, UiNodeSection, UiNodeTabBody, UiNodeView,
};
use lpc_model::{Gradient, GradientConfig};

/// One selectable palette in the chooser's lists.
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteChoice {
    /// Stable row identity: the catalog id, or a derived path for a project
    /// palette. Only used for keys and equality of provenance, never shown.
    pub id: String,
    /// The name the row prints.
    pub name: String,
    pub group: PaletteGroup,
    /// Provenance for a third-party palette: the SPDX tag rides the row,
    /// the author and source URL ride its tooltip. `None` for LightPlayer
    /// originals and project palettes — nothing to attribute.
    pub license: Option<PaletteLicense>,
    pub gradient: Gradient,
}

impl PaletteChoice {
    /// The tooltip a row carries: the attribution for a third-party palette,
    /// the group name otherwise.
    #[must_use]
    pub fn title(&self) -> String {
        match &self.license {
            Some(license) => format!(
                "{} — {} · {} · {}",
                self.name, license.author, license.spdx, license.source_url
            ),
            None => format!("{} — {}", self.name, self.group.label()),
        }
    }
}

/// The sections the chooser's list is grouped into, in presentation order:
/// what this project already uses first, then the shipped catalog by
/// provenance.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PaletteGroup {
    /// Derived from the project graph, not the catalog.
    ThisProject,
    FastledStock,
    CptCity,
    LightplayerOriginal,
}

impl PaletteGroup {
    /// Section heading for this group.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ThisProject => "This project",
            Self::FastledStock => "FastLED stock",
            Self::CptCity => "cpt-city",
            Self::LightplayerOriginal => "LightPlayer originals",
        }
    }

    /// Every group in presentation order.
    #[must_use]
    pub fn all() -> [Self; 4] {
        [
            Self::ThisProject,
            Self::FastledStock,
            Self::CptCity,
            Self::LightplayerOriginal,
        ]
    }

    fn from_category(category: PaletteCategory) -> Self {
        match category {
            PaletteCategory::FastledStock => Self::FastledStock,
            PaletteCategory::CptCity => Self::CptCity,
            PaletteCategory::LightplayerOriginal => Self::LightplayerOriginal,
        }
    }
}

/// What a chooser can offer, injected as `Signal<PaletteCatalog>` context.
///
/// `catalog: None` means "the built-in `lpa_palettes::all_palettes()`" —
/// the production case, so the workspace never copies the static catalog
/// into a signal on every view refresh. A story overrides it to pin a small
/// fake list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PaletteCatalog {
    /// Palettes already authored in this project ("This project").
    pub project: Vec<PaletteChoice>,
    /// Catalog override (stories); `None` uses the shipped catalog.
    pub catalog: Option<Vec<PaletteChoice>>,
}

impl PaletteCatalog {
    /// Every row the chooser can show, project palettes first.
    #[must_use]
    pub fn choices(&self) -> Vec<PaletteChoice> {
        let mut choices = self.project.clone();
        match &self.catalog {
            Some(catalog) => choices.extend(catalog.iter().cloned()),
            None => choices.extend(catalog_choices()),
        }
        choices
    }
}

/// The chooser's catalog: the context-provided one, or the shipped catalog
/// with no project section when nothing was provided.
pub(crate) fn use_palette_catalog() -> PaletteCatalog {
    try_consume_context::<Signal<PaletteCatalog>>()
        .map(|catalog| catalog())
        .unwrap_or_default()
}

/// The shipped catalog as chooser rows.
#[must_use]
pub fn catalog_choices() -> Vec<PaletteChoice> {
    lpa_palettes::all_palettes()
        .iter()
        .map(|entry| PaletteChoice {
            id: entry.id.clone(),
            name: entry.name.clone(),
            group: PaletteGroup::from_category(entry.category),
            license: entry.license.clone(),
            gradient: entry.gradient.clone(),
        })
        .collect()
}

/// Rows whose name matches `query` (case-insensitive substring; an empty
/// query matches everything).
#[must_use]
pub fn filter_choices(choices: &[PaletteChoice], query: &str) -> Vec<PaletteChoice> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return choices.to_vec();
    }
    choices
        .iter()
        .filter(|choice| choice.name.to_lowercase().contains(&needle))
        .cloned()
        .collect()
}

/// `choices` bucketed into the presentation groups, dropping empty ones —
/// so an empty project simply has no "This project" heading rather than an
/// empty section.
#[must_use]
pub fn group_choices(choices: &[PaletteChoice]) -> Vec<(PaletteGroup, Vec<PaletteChoice>)> {
    PaletteGroup::all()
        .into_iter()
        .filter_map(|group| {
            let rows: Vec<PaletteChoice> = choices
                .iter()
                .filter(|choice| choice.group == group)
                .cloned()
                .collect();
            (!rows.is_empty()).then_some((group, rows))
        })
        .collect()
}

/// The distinct palettes authored anywhere in `nodes`.
///
/// Walks the same UI-side data the cards render — config/debug/asset slot
/// sections, records, and nested children — so nothing is read that the user
/// cannot already see. Deduplicated by GRADIENT (the same ramp authored on
/// three nodes is one row) and capped, because this is a convenience section
/// and not an inventory.
#[must_use]
pub fn project_palette_choices(nodes: &[UiNodeView]) -> Vec<PaletteChoice> {
    /// Beyond this the section stops being a shortcut and starts being a
    /// second catalog.
    const MAX_PROJECT_PALETTES: usize = 24;

    let mut found = Vec::new();
    for node in nodes {
        collect_node(node, &mut found);
    }
    found.truncate(MAX_PROJECT_PALETTES);
    found
}

fn collect_node(node: &UiNodeView, found: &mut Vec<PaletteChoice>) {
    for tab in &node.tabs {
        if let UiNodeTabBody::Sections(sections) = &tab.body {
            collect_sections(sections, found);
        }
    }
    for child in &node.children {
        collect_child(child, found);
    }
}

fn collect_child(child: &UiNodeChild, found: &mut Vec<PaletteChoice>) {
    collect_sections(&child.sections, found);
    for nested in &child.children {
        collect_child(nested, found);
    }
}

fn collect_sections(sections: &[UiNodeSection], found: &mut Vec<PaletteChoice>) {
    for section in sections {
        match section {
            UiNodeSection::ConfigSlots(slots)
            | UiNodeSection::DebugSlots(slots)
            | UiNodeSection::AssetSlots(slots) => {
                for slot in slots {
                    collect_slot(slot, found);
                }
            }
            UiNodeSection::Children(children) => {
                for child in children {
                    collect_child(child, found);
                }
            }
            UiNodeSection::ProducedProducts(_) | UiNodeSection::ProducedValues(_) => {}
        }
    }
}

fn collect_slot(slot: &UiConfigSlot, found: &mut Vec<PaletteChoice>) {
    match &slot.body {
        UiConfigSlotBody::Value(value) => {
            let Some(config) = gradient_config_value(&value.kind.to_lp_value()) else {
                return;
            };
            push_config(&slot.label, &config, found);
        }
        UiConfigSlotBody::Record(record) => {
            for field in &record.fields {
                collect_slot(field, found);
            }
        }
        UiConfigSlotBody::Empty | UiConfigSlotBody::Asset(_) => {}
    }
}

/// Every gradient a config holds becomes its own row — a cycle's members are
/// individually pickable, which is the whole point of having them listed.
fn push_config(label: &str, config: &GradientConfig, found: &mut Vec<PaletteChoice>) {
    let gradients = config.gradients();
    let numbered = gradients.len() > 1;
    for (index, gradient) in gradients.iter().enumerate() {
        if found.iter().any(|choice| &choice.gradient == gradient) {
            continue;
        }
        let name = if numbered {
            format!("{label} {}", index + 1)
        } else {
            label.to_string()
        };
        found.push(PaletteChoice {
            id: format!("project:{name}"),
            name,
            group: PaletteGroup::ThisProject,
            license: None,
            gradient: gradient.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{UiNodeHeader, UiNodeTab, UiSlotValue};
    use lpc_model::{Colorspace, GradientStop, InterpMethod, ToLpValue};

    use super::*;

    fn ramp(shade: f32) -> Gradient {
        Gradient {
            space: Colorspace::Oklab,
            method: InterpMethod::Linear,
            stops: vec![
                GradientStop {
                    at: 0.0,
                    c: [0.0, 0.0, 0.0],
                },
                GradientStop {
                    at: 1.0,
                    c: [shade, 0.1, 0.1],
                },
            ],
        }
    }

    fn choice(name: &str, group: PaletteGroup) -> PaletteChoice {
        PaletteChoice {
            id: name.to_string(),
            name: name.to_string(),
            group,
            license: None,
            gradient: ramp(0.5),
        }
    }

    fn gradient_slot(label: &str, config: &GradientConfig) -> UiConfigSlot {
        UiConfigSlot::value(
            label,
            label,
            UiSlotValue::from_lp_value(&config.to_lp_value()),
        )
    }

    fn nodes_with_slots(slots: Vec<UiConfigSlot>) -> Vec<UiNodeView> {
        vec![UiNodeView::new(
            UiNodeHeader::new("/show.module", "show", "module"),
            vec![UiNodeTab::main(vec![UiNodeSection::ConfigSlots(slots)])],
        )]
    }

    #[test]
    fn the_shipped_catalog_lands_in_provenance_groups() {
        let choices = catalog_choices();
        assert!(!choices.is_empty());
        // Third-party rows carry their license; originals must not.
        for choice in &choices {
            match choice.group {
                PaletteGroup::FastledStock | PaletteGroup::CptCity => {
                    assert!(choice.license.is_some(), "{} needs a license", choice.name);
                }
                PaletteGroup::LightplayerOriginal => assert!(choice.license.is_none()),
                PaletteGroup::ThisProject => panic!("the catalog is never a project palette"),
            }
        }
    }

    #[test]
    fn search_matches_names_case_insensitively() {
        let rows = vec![
            choice("Ocean", PaletteGroup::FastledStock),
            choice("Lava", PaletteGroup::FastledStock),
        ];
        assert_eq!(filter_choices(&rows, "oce").len(), 1);
        assert_eq!(filter_choices(&rows, "  ").len(), 2);
        assert!(filter_choices(&rows, "nothing").is_empty());
    }

    #[test]
    fn grouping_drops_empty_sections() {
        let rows = vec![
            choice("Ocean", PaletteGroup::FastledStock),
            choice("Dusk", PaletteGroup::LightplayerOriginal),
        ];
        let groups = group_choices(&rows);
        assert_eq!(
            groups.iter().map(|(group, _)| *group).collect::<Vec<_>>(),
            vec![
                PaletteGroup::FastledStock,
                PaletteGroup::LightplayerOriginal
            ],
            "an empty project section is absent, not empty"
        );
    }

    #[test]
    fn the_project_section_is_derived_from_authored_slot_values() {
        let cycle = GradientConfig::Cycle {
            set: vec![ramp(0.2), ramp(0.7)],
            step_seconds: 20.0,
            fade_seconds: 0.5,
        };
        let nodes = nodes_with_slots(vec![
            gradient_slot("Palette", &GradientConfig::Static(ramp(0.9))),
            gradient_slot("Cycle", &cycle),
            // A second authoring of an existing ramp is the SAME palette.
            gradient_slot("Copy", &GradientConfig::Static(ramp(0.9))),
            UiConfigSlot::value("speed", "speed", UiSlotValue::f32(1.5)),
        ]);

        let names: Vec<String> = project_palette_choices(&nodes)
            .into_iter()
            .map(|choice| choice.name)
            .collect();
        assert_eq!(names, vec!["Palette", "Cycle 1", "Cycle 2"]);
    }

    #[test]
    fn an_empty_project_derives_no_palettes() {
        assert!(project_palette_choices(&[]).is_empty());
        assert!(project_palette_choices(&nodes_with_slots(Vec::new())).is_empty());
    }

    #[test]
    fn a_catalog_override_replaces_only_the_catalog_half() {
        let catalog = PaletteCatalog {
            project: vec![choice("Mine", PaletteGroup::ThisProject)],
            catalog: Some(vec![choice("Ocean", PaletteGroup::FastledStock)]),
        };
        let names: Vec<String> = catalog
            .choices()
            .into_iter()
            .map(|choice| choice.name)
            .collect();
        assert_eq!(names, vec!["Mine", "Ocean"]);

        // No override: the shipped catalog rides behind the project rows.
        let live = PaletteCatalog {
            project: vec![choice("Mine", PaletteGroup::ThisProject)],
            catalog: None,
        };
        assert!(live.choices().len() > 1);
    }

    #[test]
    fn third_party_rows_put_the_attribution_in_their_tooltip() {
        let mut row = choice("Rainfall", PaletteGroup::CptCity);
        row.license = Some(PaletteLicense {
            spdx: "CC-BY-3.0".to_string(),
            author: "jjg".to_string(),
            source_url: "http://soliton.vm.bytemark.co.uk/".to_string(),
        });
        let title = row.title();
        assert!(title.contains("jjg"));
        assert!(title.contains("CC-BY-3.0"));
        assert!(title.contains("soliton"));
    }
}
