//! A panel's channel list, and the nested groups it presents beneath it.
//!
//! `docs/design/modules.md` R8: a module's panel presents its scope's
//! channel list — the aggregate of its children's publicity — plus each
//! child module's panel as a **nested group**. The recursion is
//! presentation only: nothing is promoted, no dataflow construct sits
//! behind a group, and two embedded instances of the same effect present
//! two independent groups because they are two different scopes.

use crate::{
    UiPanelControlView, UiSlotAffordance, UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow,
};

/// One panel: the channels of one scope, plus its child modules' panels.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiPanelGroup {
    /// Group heading — the module's own name. The root module's panel wears
    /// the project name; a nested group wears the embedded module's.
    pub label: String,
    /// The scope this group presents, as a node path (`/`, `/plasma-1`).
    /// Together with a channel name this is a control's identity (panel.md
    /// P1) — the DISPLAY half; the dispatchable half is [`Self::target`].
    pub scope: String,
    /// The structured scope the group's reset gesture clears
    /// (`WirePanelClearRequest::Scope`). `None` on story fixtures with no
    /// runtime behind them — the reset affordance simply doesn't render.
    pub target: Option<lpc_wire::WireScopeRef>,
    /// The scope's channels, in listing order.
    pub controls: Vec<UiPanelControlView>,
    /// Child modules' panels — presentation recursion (R8).
    ///
    /// Groups render as bordered clusters in a wrapping row and are
    /// **always open**: wrapping is the density mechanism, not disclosure.
    /// A panel you have to unfold before you can play it is not a control
    /// panel, so there is deliberately no collapsed state here.
    pub groups: Vec<UiPanelGroup>,
}

impl UiPanelGroup {
    /// An empty panel for `scope`, labeled `label`.
    pub fn new(label: impl Into<String>, scope: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            scope: scope.into(),
            target: None,
            controls: Vec::new(),
            groups: Vec::new(),
        }
    }

    /// Attach the structured scope the reset gesture dispatches against.
    pub fn with_target(mut self, target: lpc_wire::WireScopeRef) -> Self {
        self.target = Some(target);
        self
    }

    /// Add this scope's own channels.
    pub fn with_controls(mut self, controls: Vec<UiPanelControlView>) -> Self {
        self.controls = controls;
        self
    }

    /// Add nested child-module groups.
    pub fn with_groups(mut self, groups: Vec<UiPanelGroup>) -> Self {
        self.groups = groups;
        self
    }

    /// Whether the group presents nothing at all — no channels here and
    /// nothing in any nested group.
    pub fn is_empty(&self) -> bool {
        self.controls.is_empty() && self.groups.iter().all(Self::is_empty)
    }

    /// Engaged controls in this group only — what the group's own reset
    /// gesture would clear if it did not descend.
    pub fn engaged_here(&self) -> usize {
        self.controls
            .iter()
            .filter(|control| control.state.engaged())
            .count()
    }

    /// Engaged controls in this group and every nested group — what "reset
    /// this module" clears (panel.md P2 clear at scope granularity; P-Q4's
    /// lean is that a clear descends).
    pub fn engaged_total(&self) -> usize {
        self.engaged_here() + self.groups.iter().map(Self::engaged_total).sum::<usize>()
    }

    /// Collapsed-row summary: how many controls the group holds and how
    /// many of them are currently held.
    pub fn summary(&self) -> String {
        let controls = self.controls.len();
        let noun = if controls == 1 { "control" } else { "controls" };
        match self.engaged_total() {
            0 => format!("{controls} {noun}"),
            engaged => format!("{controls} {noun} · {engaged} held"),
        }
    }

    /// The group heading's detail-popup sections.
    ///
    /// A nested group's heading is a hairline rule with its name on it and
    /// nothing else — the scope path and the control tally are identity and
    /// bookkeeping, not things to read while playing, so they live here,
    /// behind the same label-popup gesture the controls use.
    pub fn detail_aspects(&self) -> Vec<UiSlotAspect> {
        let held = self.engaged_total();
        let state = if held == 0 {
            UiSlotAspect::new(UiSlotAspectKind::PanelState, "Nothing held").with_row(
                UiSlotAspectRow::new("", "Every control in this group follows the project."),
            )
        } else {
            let clause = if held == 1 {
                "1 control in this group is held by the panel.".to_string()
            } else {
                format!("{held} controls in this group are held by the panel.")
            };
            UiSlotAspect::new(UiSlotAspectKind::PanelState, "Held")
                .with_affordance(UiSlotAffordance::Edited)
                .with_row(UiSlotAspectRow::new("", clause))
        };

        vec![
            state,
            UiSlotAspect::new(UiSlotAspectKind::TypeInfo, "Group")
                .with_row(UiSlotAspectRow::new("Name", self.label.clone()))
                .with_row(UiSlotAspectRow::new("Scope", self.scope.clone()))
                .with_row(UiSlotAspectRow::new("Controls", self.summary())),
        ]
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        UiPanelControl, UiPanelControlState, UiPanelControlView, UiPanelGroup, UiPanelWidget,
        UiSlotFieldState, UiSlotValue,
    };

    fn control(channel: &str, state: UiPanelControlState) -> UiPanelControlView {
        UiPanelControlView {
            channel: channel.to_string(),
            control: UiPanelControl {
                label: channel.to_string(),
                address: None,
                widget: UiPanelWidget::Knob {
                    min: 0.0,
                    max: 1.0,
                    step: None,
                },
                value: UiSlotValue::f32(0.5),
                emit: crate::UiPanelEmit::Value,
                live_value: None,
                live_gradient: None,
                panel_target: None,
                unit: None,
                state: UiSlotFieldState::editable(),
                aspects: Vec::new(),
                wires: Vec::new(),
            },
            state,
            source: None,
        }
    }

    fn plasma(scope: &str, state: UiPanelControlState) -> UiPanelGroup {
        UiPanelGroup::new("plasma", scope).with_controls(vec![control("speed", state)])
    }

    #[test]
    fn a_module_reset_counts_every_nested_group() {
        let panel = UiPanelGroup::new("Aurora", "/")
            .with_controls(vec![
                control("brightness", UiPanelControlState::Engaged),
                control("time", UiPanelControlState::ReadFollowing),
            ])
            .with_groups(vec![
                plasma("/plasma-1", UiPanelControlState::Engaged),
                plasma("/plasma-2", UiPanelControlState::ReadDefault),
            ]);

        // The root's own reset would clear one; resetting the module clears
        // the nested plasma-1 writer too.
        assert_eq!(panel.engaged_here(), 1);
        assert_eq!(panel.engaged_total(), 2);
        // Two instances of one effect are two independent groups (R8).
        assert_eq!(panel.groups[0].engaged_total(), 1);
        assert_eq!(panel.groups[1].engaged_total(), 0);
    }

    #[test]
    fn summary_reports_held_controls_only_when_there_are_some() {
        let quiet = plasma("/plasma-2", UiPanelControlState::ReadDefault);
        assert_eq!(quiet.summary(), "1 control");

        let held = plasma("/plasma-1", UiPanelControlState::Engaged);
        assert_eq!(held.summary(), "1 control · 1 held");
    }

    #[test]
    fn emptiness_looks_through_nested_groups() {
        let hollow =
            UiPanelGroup::new("Aurora", "/").with_groups(vec![UiPanelGroup::new("plasma", "/p")]);
        assert!(hollow.is_empty());

        let filled = UiPanelGroup::new("Aurora", "/")
            .with_groups(vec![plasma("/p", UiPanelControlState::ReadDefault)]);
        assert!(!filled.is_empty());
    }
}
