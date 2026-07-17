//! A rich object's ordered sections and the derived rollup.

use crate::UiStatusKind;

use super::rich_section::{RichSection, RichWeight};

/// A rich object's detail: the ordered section list plus the derived
/// rollup. Sections stay exactly in the order given (the fixed schema
/// order, Q4 — never worst-first); builders pin any danger section last.
#[derive(Clone, Debug, PartialEq)]
pub struct RichObjectView<A> {
    pub sections: Vec<RichSection<A>>,
}

/// The object-level derivation every surface consumes (the two rollup
/// rules): the indicator wears the worst ACTIONABLE section's tone, and
/// that same section's affordance is the object's primary affordance. No
/// surface picks its own winner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RichRollup<'a, A> {
    /// The worst actionable section's tone; `Neutral` when no actionable
    /// section exists. Advisory and Danger sections never color this.
    pub tone: UiStatusKind,
    /// The winning section's (first) affordance, if it carries one.
    pub affordance: Option<&'a A>,
}

impl<A> RichObjectView<A> {
    pub fn new(sections: Vec<RichSection<A>>) -> Self {
        Self { sections }
    }

    /// Derive the rollup: scan the ACTIONABLE sections for the worst tone
    /// (ties resolve to the earlier section, so the schema order is also
    /// the tie-break precedence — Health outranks Project at equal
    /// severity). Advisory and Danger sections are invisible here.
    pub fn rollup(&self) -> RichRollup<'_, A> {
        // `min_by_key` keeps the FIRST minimum, so reversing the severity
        // makes ties resolve to the earlier section.
        let winner = self
            .sections
            .iter()
            .filter(|section| section.weight == RichWeight::Actionable)
            .min_by_key(|section| core::cmp::Reverse(tone_severity(section.tone)));
        RichRollup {
            tone: winner.map_or(UiStatusKind::Neutral, |section| section.tone),
            affordance: winner.and_then(|section| section.affordances.first()),
        }
    }
}

/// Worst-first rank of a status family for the rollup.
fn tone_severity(tone: UiStatusKind) -> u8 {
    match tone {
        UiStatusKind::Neutral => 0,
        UiStatusKind::Good => 1,
        UiStatusKind::Working => 2,
        UiStatusKind::Warning => 3,
        UiStatusKind::Error => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::super::rich_section::RichLine;
    use super::*;

    #[test]
    fn worst_actionable_section_owns_tone_and_affordance() {
        let view = RichObjectView::new(vec![
            section(
                "Health",
                UiStatusKind::Good,
                RichWeight::Actionable,
                &["open"],
            ),
            section(
                "Project",
                UiStatusKind::Warning,
                RichWeight::Actionable,
                &["push"],
            ),
            section(
                "Backup",
                UiStatusKind::Neutral,
                RichWeight::Actionable,
                &["download"],
            ),
        ]);
        let rollup = view.rollup();
        assert_eq!(rollup.tone, UiStatusKind::Warning);
        assert_eq!(rollup.affordance, Some(&"push"));
    }

    #[test]
    fn severity_ties_resolve_to_the_earlier_section() {
        // Health and Project both Warning: the schema order is the
        // precedence, so Health's affordance stays primary.
        let view = RichObjectView::new(vec![
            section(
                "Health",
                UiStatusKind::Warning,
                RichWeight::Actionable,
                &["troubleshoot"],
            ),
            section(
                "Project",
                UiStatusKind::Warning,
                RichWeight::Actionable,
                &["push"],
            ),
        ]);
        assert_eq!(view.rollup().affordance, Some(&"troubleshoot"));
    }

    #[test]
    fn advisory_and_danger_sections_never_color_the_rollup() {
        // An Error-toned advisory fact and a Danger section must not
        // outshout a Good actionable section.
        let view = RichObjectView::new(vec![
            section(
                "Health",
                UiStatusKind::Good,
                RichWeight::Actionable,
                &["open"],
            ),
            section("Technical", UiStatusKind::Error, RichWeight::Advisory, &[]),
            section(
                "Danger zone",
                UiStatusKind::Error,
                RichWeight::Danger,
                &["erase"],
            ),
        ]);
        let rollup = view.rollup();
        assert_eq!(rollup.tone, UiStatusKind::Good);
        assert_eq!(rollup.affordance, Some(&"open"));
    }

    #[test]
    fn no_actionable_section_rolls_up_neutral_and_quiet() {
        let view = RichObjectView::new(vec![
            section(
                "Technical",
                UiStatusKind::Warning,
                RichWeight::Advisory,
                &[],
            ),
            section(
                "Danger zone",
                UiStatusKind::Neutral,
                RichWeight::Danger,
                &["erase"],
            ),
        ]);
        let rollup = view.rollup();
        assert_eq!(rollup.tone, UiStatusKind::Neutral);
        assert_eq!(rollup.affordance, None);
    }

    #[test]
    fn sections_keep_their_construction_order() {
        // Q4: fixed schema order — the view never sorts, even when a
        // worse section sits later in the list.
        let view = RichObjectView::new(vec![
            section("Health", UiStatusKind::Neutral, RichWeight::Actionable, &[]),
            section(
                "Project",
                UiStatusKind::Error,
                RichWeight::Actionable,
                &["resolve"],
            ),
            section(
                "Danger zone",
                UiStatusKind::Neutral,
                RichWeight::Danger,
                &[],
            ),
        ]);
        let titles: Vec<&str> = view
            .sections
            .iter()
            .map(|section| section.title.as_str())
            .collect();
        assert_eq!(titles, vec!["Health", "Project", "Danger zone"]);
        // …while the rollup still finds the worst actionable section.
        assert_eq!(view.rollup().tone, UiStatusKind::Error);
    }

    fn section(
        title: &str,
        tone: UiStatusKind,
        weight: RichWeight,
        affordances: &[&'static str],
    ) -> RichSection<&'static str> {
        RichSection {
            title: title.to_string(),
            tone,
            lines: vec![RichLine::new("label", "value")],
            chip: None,
            affordances: affordances.to_vec(),
            weight,
        }
    }
}
