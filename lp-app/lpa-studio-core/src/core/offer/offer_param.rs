//! [`OfferParam`]: one value an offer needs before it can be pressed —
//! which board to flash, what to call it.

use crate::OfferArgError;

/// One value an offer takes when it is pressed.
///
/// The offer declares what it needs and core fills in what it already
/// knows: a choice's options are the list core already builds (the boards a
/// detected chip fits), never something a renderer invents. A renderer
/// draws the parameter as a picker, a field or a switch; the app agent
/// fills it by `name`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfferParam {
    /// The key a press carries the value under (`board`, `name`).
    pub name: String,
    /// What the user calls it, lower case (`board`, `name`): renderers
    /// label the control with it, and an unbound offer says "choose a
    /// <label>".
    pub label: String,
    /// What kind of value it is, and what is allowed.
    pub kind: OfferParamKind,
    /// One plain line about the whole parameter, when there is something
    /// to say (a version list that is shorter than usual because the full
    /// one cannot be read): renderers draw it under the control.
    pub note: Option<String>,
}

/// The kinds of value an offer can take.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OfferParamKind {
    /// One of a closed list core computed.
    Choice {
        options: Vec<OfferChoice>,
        /// The option picked for the user when there is an obvious one (a
        /// detected chip with exactly one board). A press that leaves the
        /// parameter out gets this value.
        preselect: Option<String>,
    },
    /// Free text.
    Text {
        /// What an empty field shows: what happens when it is left blank.
        placeholder: String,
        /// The most characters allowed, when there is a limit.
        max_len: Option<usize>,
        /// Whether the press may leave it out.
        optional: bool,
        /// A secret (a Wi‑Fi password): drawn as a password field, never
        /// echoed. A press's stamp carries [`crate::SECRET_MARKER`] in its
        /// place ([`crate::UiOffer::press`]), and the app agent never fills
        /// one — an offer that takes a secret is always the user's card.
        secret: bool,
    },
    /// On or off.
    Toggle {
        /// The current state: a press that leaves the parameter out keeps
        /// it.
        value: bool,
    },
}

/// One option of a [`OfferParamKind::Choice`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfferChoice {
    /// What a press carries (`seeed-xiao-esp32c6`).
    pub value: String,
    /// What the user reads (`Seeed XIAO ESP32-C6`).
    pub label: String,
    /// One more line about it, when there is one.
    pub detail: Option<String>,
    /// A caution about picking it (a version in an older language than
    /// this Studio), drawn in the warning tone under the detail.
    pub warning: Option<String>,
    /// Why it cannot be picked right now, when it cannot. Drawn disabled
    /// with this reason, never hidden.
    pub disabled: Option<String>,
    /// The toggle parameter that widens the choice to this option, when it
    /// is one the list is narrowed away from by default: a board outside
    /// the detected chip is offered only with `all_boards` on. A renderer
    /// draws it only while that toggle is on, and a press that picks it
    /// with the toggle off is refused ([`crate::UiOffer::press`]).
    pub only_with: Option<String>,
}

impl OfferParam {
    /// A choice among `options`, with an optional `preselect`.
    pub fn choice(
        name: impl Into<String>,
        label: impl Into<String>,
        options: Vec<OfferChoice>,
        preselect: Option<String>,
    ) -> Self {
        Self {
            name: name.into(),
            label: label.into(),
            note: None,
            kind: OfferParamKind::Choice { options, preselect },
        }
    }

    /// Free text, required, with no length limit; see [`Self::optional`]
    /// and [`Self::max_len`].
    pub fn text(
        name: impl Into<String>,
        label: impl Into<String>,
        placeholder: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            label: label.into(),
            note: None,
            kind: OfferParamKind::Text {
                placeholder: placeholder.into(),
                max_len: None,
                optional: false,
                secret: false,
            },
        }
    }

    /// An on/off switch whose current state is `value`.
    pub fn toggle(name: impl Into<String>, label: impl Into<String>, value: bool) -> Self {
        Self {
            name: name.into(),
            label: label.into(),
            note: None,
            kind: OfferParamKind::Toggle { value },
        }
    }

    /// With `note` drawn under the control.
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }

    /// Text the press may leave out. No effect on other kinds.
    pub fn optional(mut self) -> Self {
        if let OfferParamKind::Text { optional, .. } = &mut self.kind {
            *optional = true;
        }
        self
    }

    /// Text that is a secret (the `secret` field of [`OfferParamKind::Text`]). No
    /// effect on other kinds.
    pub fn secret(mut self) -> Self {
        if let OfferParamKind::Text { secret, .. } = &mut self.kind {
            *secret = true;
        }
        self
    }

    /// Whether this is secret text.
    pub fn is_secret(&self) -> bool {
        matches!(self.kind, OfferParamKind::Text { secret: true, .. })
    }

    /// Text of at most `limit` characters. No effect on other kinds.
    pub fn max_len(mut self, limit: usize) -> Self {
        if let OfferParamKind::Text { max_len, .. } = &mut self.kind {
            *max_len = Some(limit);
        }
        self
    }

    /// The value a press that leaves this parameter out gets: a choice's
    /// preselect, a toggle's current state. `None` for text, and for a
    /// choice with nothing preselected.
    pub fn default_value(&self) -> Option<String> {
        match &self.kind {
            OfferParamKind::Choice { preselect, .. } => preselect.clone(),
            OfferParamKind::Toggle { value } => Some(value.to_string()),
            OfferParamKind::Text { .. } => None,
        }
    }

    /// Whether a press must carry a value (directly or through
    /// [`Self::default_value`]).
    pub fn is_required(&self) -> bool {
        !matches!(self.kind, OfferParamKind::Text { optional: true, .. })
    }

    /// Check one value against what this parameter allows: a choice must be
    /// one of its options and not disabled, text (trimmed) must fit
    /// `max_len` and, when required, not be blank, a toggle must be `true`
    /// or `false`.
    pub fn check(&self, value: &str) -> Result<(), OfferArgError> {
        match &self.kind {
            OfferParamKind::Choice { options, .. } => {
                let Some(option) = options.iter().find(|option| option.value == value) else {
                    return Err(OfferArgError::NotAnOption {
                        name: self.name.clone(),
                        value: value.to_string(),
                        options: options.iter().map(|option| option.value.clone()).collect(),
                    });
                };
                match &option.disabled {
                    Some(reason) => Err(OfferArgError::OptionDisabled {
                        name: self.name.clone(),
                        value: value.to_string(),
                        reason: reason.clone(),
                    }),
                    None => Ok(()),
                }
            }
            // Text is read trimmed (`OfferArgs::text`), so it is measured
            // trimmed, and a blank required field is a missing one.
            OfferParamKind::Text {
                max_len, optional, ..
            } => {
                let text = value.trim();
                if text.is_empty() && !optional {
                    return Err(OfferArgError::Missing {
                        name: self.name.clone(),
                        label: self.label.clone(),
                    });
                }
                match max_len {
                    Some(limit) if text.chars().count() > *limit => Err(OfferArgError::TooLong {
                        name: self.name.clone(),
                        max_len: *limit,
                    }),
                    _ => Ok(()),
                }
            }
            OfferParamKind::Toggle { .. } => match value {
                "true" | "false" => Ok(()),
                _ => Err(OfferArgError::NotAToggle {
                    name: self.name.clone(),
                    value: value.to_string(),
                }),
            },
        }
    }
}

impl OfferChoice {
    /// An option carrying `value`, read as `label`.
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            detail: None,
            warning: None,
            disabled: None,
            only_with: None,
        }
    }

    /// One more line about it.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// With a caution about picking it.
    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warning = Some(warning.into());
        self
    }

    /// Drawn but not pickable, for `reason`.
    pub fn disabled(mut self, reason: impl Into<String>) -> Self {
        self.disabled = Some(reason.into());
        self
    }

    /// Offered only while the toggle parameter `toggle` is on.
    pub fn only_with(mut self, toggle: impl Into<String>) -> Self {
        self.only_with = Some(toggle.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boards() -> OfferParam {
        OfferParam::choice(
            "board",
            "board",
            vec![
                OfferChoice::new("xiao", "XIAO ESP32-C6").with_detail("Seeed · 4 MB flash"),
                OfferChoice::new("devkit", "ESP32-C6 DevKit").disabled("no build served"),
            ],
            Some("xiao".to_string()),
        )
    }

    #[test]
    fn a_choice_accepts_only_its_enabled_options() {
        let board = boards();
        assert_eq!(board.check("xiao"), Ok(()));
        assert_eq!(
            board.check("nope"),
            Err(OfferArgError::NotAnOption {
                name: "board".to_string(),
                value: "nope".to_string(),
                options: vec!["xiao".to_string(), "devkit".to_string()],
            })
        );
        assert!(matches!(
            board.check("devkit"),
            Err(OfferArgError::OptionDisabled { reason, .. }) if reason == "no build served"
        ));
        assert_eq!(board.default_value().as_deref(), Some("xiao"));
        assert!(board.is_required());
    }

    #[test]
    fn text_fits_its_limit_and_may_be_optional() {
        let name = OfferParam::text("name", "name", "Named after the board").max_len(5);
        assert!(name.is_required());
        assert_eq!(name.check("Desk"), Ok(()));
        assert_eq!(
            name.check("Kitchen"),
            Err(OfferArgError::TooLong {
                name: "name".to_string(),
                max_len: 5
            })
        );
        assert_eq!(name.check("Desk·"), Ok(()), "characters, not bytes");
        assert_eq!(name.check("  Desk  "), Ok(()), "measured trimmed");
        assert!(matches!(
            name.check("   "),
            Err(OfferArgError::Missing { .. })
        ));
        let name = name.optional();
        assert!(!name.is_required());
        assert_eq!(name.check(""), Ok(()), "an optional field may be blank");
    }

    #[test]
    fn only_text_can_be_secret() {
        let password = OfferParam::text("password", "password", "unchanged")
            .optional()
            .secret();
        assert!(password.is_secret());
        assert!(!password.is_required());
        assert!(!OfferParam::text("name", "name", "").is_secret());
        assert!(
            !OfferParam::toggle("enabled", "on", true)
                .secret()
                .is_secret(),
            "no effect on a toggle"
        );
    }

    #[test]
    fn a_toggle_reads_true_or_false_and_defaults_to_its_state() {
        let auto = OfferParam::toggle("enabled", "autoconnect", true);
        assert_eq!(auto.default_value().as_deref(), Some("true"));
        assert_eq!(auto.check("false"), Ok(()));
        assert!(matches!(
            auto.check("yes"),
            Err(OfferArgError::NotAToggle { .. })
        ));
    }
}
