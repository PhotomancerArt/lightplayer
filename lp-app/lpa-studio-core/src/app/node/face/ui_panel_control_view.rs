//! One control on a **panel**, plus the panel state behind it.
//!
//! `docs/design/panel.md` is the normative treatment. A control is
//! identified by `(scope, channel)` (P1); the scope is the owning
//! [`crate::UiPanelGroup`], the channel is [`UiPanelControlView::channel`].
//! The widget payload is the existing [`UiPanelControl`] verbatim — the
//! module model reuses the node-face panel widgets rather than forking a
//! second control family.
//!
//! M2 UX spike: the state below is carried by mock fixtures. The engine
//! runtime that materializes real panel writers is M4's.

use crate::{
    UiPanelControl, UiSlotAffordance, UiSlotAspect, UiSlotAspectKind, UiSlotAspectRow,
};

/// The three visibly distinct states a panel control can be in
/// (`docs/design/panel.md` P2; the three-state requirement is P-Q2).
///
/// These are *not* a severity ladder and must not reuse the bound-violet
/// family: "bound" means wired, "engaged" means captured (P6).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum UiPanelControlState {
    /// **Read, at default.** No writer anywhere in the scope chain, so the
    /// consuming slot falls back to its own authored default
    /// (`modules.md` R6). The channel still lists — an unfilled public
    /// input is an invitation, not an error.
    #[default]
    ReadDefault,
    /// **Read, following automation.** Some writer that is *not* this
    /// control's drives the channel: an authored node in this scope (an
    /// LFO, a clock) or an inherited writer from an enclosing scope
    /// (`modules.md` R5). The control displays the live value and can be
    /// grabbed (P5 jump takeover).
    ReadFollowing,
    /// **Engaged (Latch).** This control's panel writer exists and holds,
    /// shadowing other writers in its scope until cleared (P2/P4).
    Engaged,
}

impl UiPanelControlState {
    /// Whether the control's panel writer exists — the one bit a reset
    /// gesture acts on (P2 clear).
    pub fn engaged(self) -> bool {
        matches!(self, Self::Engaged)
    }

    /// Short state word for the control's caption and for the group's
    /// summary rows.
    pub fn word(self) -> &'static str {
        match self {
            Self::ReadDefault => "default",
            Self::ReadFollowing => "following",
            Self::Engaged => "held",
        }
    }
}

/// One channel presented on one panel.
#[derive(Clone, Debug, PartialEq)]
pub struct UiPanelControlView {
    /// The channel name within the owning group's scope — the other half of
    /// the control's identity (P1).
    pub channel: String,
    /// Widget, label, value, unit, and detail aspects. Reused verbatim from
    /// the node-face panel so knob v2, the fader, and the toggle behave
    /// identically wherever they appear.
    pub control: UiPanelControl,
    /// Which of the three panel states this control is in.
    pub state: UiPanelControlState,
    /// Who owns the displayed value while the control is in Read: "clock",
    /// "inherited from Evening set", "authored default" (P2 — the UI
    /// distinguishes inherited / authored / default).
    pub source: Option<String>,
}

impl UiPanelControlView {
    /// A control in its Read-at-default state.
    pub fn new(channel: impl Into<String>, control: UiPanelControl) -> Self {
        Self {
            channel: channel.into(),
            control,
            state: UiPanelControlState::ReadDefault,
            source: None,
        }
    }

    /// Put the control in a state, with the Read caption that explains it.
    pub fn with_state(
        mut self,
        state: UiPanelControlState,
        source: Option<impl Into<String>>,
    ) -> Self {
        self.state = state;
        self.source = source.map(Into::into);
        self
    }

    /// The control's detail-popup sections.
    ///
    /// A control on the face is widget + label + value and nothing else —
    /// a control panel, not a spec sheet. Everything that used to sit under
    /// it as a caption lives here instead, behind the label: which of the
    /// three states it is in, what drives it, what a held value displaced,
    /// and the `(scope, channel)` identity itself (P1).
    ///
    /// The widget's own aspects (validation, type info, binding on the
    /// backing slot) follow, so the panel popup is a superset of the slot
    /// popup rather than a fork of it.
    pub fn detail_aspects(&self, scope: &str) -> Vec<UiSlotAspect> {
        let mut aspects = vec![self.state_aspect()];
        aspects.push(
            UiSlotAspect::new(UiSlotAspectKind::TypeInfo, "Channel")
                .with_row(UiSlotAspectRow::new("Name", self.control.label.clone()))
                .with_row(UiSlotAspectRow::new("Channel", self.channel.clone()))
                .with_row(UiSlotAspectRow::new("Scope", scope)),
        );
        aspects.extend(self.control.aspects.iter().cloned());
        aspects
    }

    /// The panel-state section: one title per state, and rows that say what
    /// the state displaced.
    fn state_aspect(&self) -> UiSlotAspect {
        let source = self.source.clone();
        match self.state {
            UiPanelControlState::Engaged => {
                let aspect = UiSlotAspect::new(UiSlotAspectKind::PanelState, "Held")
                    .with_affordance(UiSlotAffordance::Edited)
                    .with_row(UiSlotAspectRow::new(
                        "",
                        "This panel holds the channel; other writers are shadowed until it is reset.",
                    ));
                match source {
                    Some(source) => aspect.with_row(UiSlotAspectRow::new("Was", source)),
                    None => aspect,
                }
            }
            UiPanelControlState::ReadFollowing => {
                let aspect = UiSlotAspect::new(UiSlotAspectKind::PanelState, "Following")
                    .with_affordance(UiSlotAffordance::Bound)
                    .with_row(UiSlotAspectRow::new(
                        "",
                        "Something else drives this channel; turning the control takes it over.",
                    ));
                match source {
                    Some(source) => aspect.with_row(UiSlotAspectRow::new("Driven by", source)),
                    None => aspect,
                }
            }
            UiPanelControlState::ReadDefault => {
                let aspect = UiSlotAspect::new(UiSlotAspectKind::PanelState, "At default")
                    .with_row(UiSlotAspectRow::new(
                        "",
                        "Nothing writes this channel, so the consuming slot uses its own default.",
                    ));
                match source {
                    Some(source) => aspect.with_row(UiSlotAspectRow::new("Value from", source)),
                    None => aspect,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UiPanelControlState;

    #[test]
    fn only_the_engaged_state_carries_a_panel_writer() {
        assert!(UiPanelControlState::Engaged.engaged());
        assert!(!UiPanelControlState::ReadFollowing.engaged());
        assert!(!UiPanelControlState::ReadDefault.engaged());
        // Read-at-default is the resting state a fresh panel renders in.
        assert_eq!(
            UiPanelControlState::default(),
            UiPanelControlState::ReadDefault
        );
    }
}
