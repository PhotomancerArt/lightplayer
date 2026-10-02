//! [`UiOffer`]: one verb the user can press, at its path — and, when it
//! needs values first, the parameters it takes.

use crate::{
    ActionConsequence, ActionEnablement, ActionPriority, OfferArgError, OfferArgs, OfferBinder,
    OfferParam, OfferParamKind, OfferPath, UiAction,
};

/// One verb the user can press, addressed by a stable [`OfferPath`].
///
/// The icon is required (header surfaces render icon buttons). Everything
/// else a renderer or the agent needs (label, summary, emphasis,
/// enablement, consequence) is read from the wrapped action's
/// [`crate::ActionMeta`], so the action stays the one source of that
/// metadata.
///
/// # Offers that take values
///
/// A verb that needs a value first (flash *which board*) declares
/// [`OfferParam`]s and carries an [`OfferBinder`] that turns a press's
/// [`OfferArgs`] into the action to dispatch. [`Self::press`] checks the
/// values against the parameters, fills defaults, and binds.
///
/// `action` is still always set, because it is how every consumer renders
/// the verb (label, level, enablement): for a parameterised offer it is the
/// action bound with the defaults (preselected choices, toggles' current
/// state). When the defaults do not bind — a choice with nothing
/// preselected — it is the `unbound` action the publisher handed over,
/// disabled with "choose a <label>". Dispatching `action` directly is
/// therefore always what a press with no values would do.
///
/// Equality compares the path, the action (its operation and render
/// metadata), the icon and the parameters, never the binder's closure.
#[derive(Clone, Debug)]
pub struct UiOffer {
    /// Where the offer lives: `project/save`,
    /// `project/demo.module/orbit.shader/remove`.
    pub path: OfferPath,
    /// The dispatchable controller operation plus its render metadata.
    pub action: UiAction,
    /// Icon token understood by the renderer (same vocabulary as
    /// `ActionMeta::icon`, e.g. `"save"`).
    pub icon: String,
    params: Vec<OfferParam>,
    binder: Option<OfferBinder>,
}

impl PartialEq for UiOffer {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
            && self.action == other.action
            && self.icon == other.icon
            && self.params == other.params
    }
}

impl Eq for UiOffer {}

impl UiOffer {
    /// An offer at `path`, drawn with `icon`, that dispatches `action`.
    pub fn new(path: OfferPath, icon: impl Into<String>, action: UiAction) -> Self {
        Self {
            path,
            action,
            icon: icon.into(),
            params: Vec::new(),
            binder: None,
        }
    }

    /// An offer that takes `params`, turned into its action by `binder`.
    ///
    /// `unbound` is how the verb reads when its defaults do not bind (see
    /// the type's docs): its label, summary and level are the verb's, and
    /// this disables it with the reason. Give it the same consequence the
    /// binder gives its actions, so the verb reads at the same level before
    /// and after a choice.
    pub fn with_params(
        path: OfferPath,
        icon: impl Into<String>,
        params: Vec<OfferParam>,
        binder: OfferBinder,
        unbound: UiAction,
    ) -> Self {
        let mut offer = Self {
            path,
            action: unbound,
            icon: icon.into(),
            params,
            binder: Some(binder),
        };
        offer.action = match offer.press_unchecked(&OfferArgs::new()) {
            Ok(bound) => bound,
            Err(OfferArgError::Missing { label, .. }) => {
                offer.action.clone().disabled(format!("choose a {label}"))
            }
            Err(error) => offer.action.clone().disabled(error.to_string()),
        };
        offer
    }

    /// The values a press must (or may) carry, in the order a renderer
    /// draws them. Empty for an ordinary one-click verb.
    pub fn params(&self) -> &[OfferParam] {
        &self.params
    }

    /// The action one press with `args` dispatches, or why the press is
    /// refused.
    ///
    /// - A parameterless offer accepts empty args and refuses any other.
    /// - A parameterised one refuses a name it does not take, checks each
    ///   value (a choice must be an enabled option, text must fit its
    ///   limit, a toggle must be `true`/`false`), fills what was left out
    ///   from the defaults, refuses a required parameter still missing,
    ///   then binds.
    /// - Either way, an action that comes out disabled is refused as
    ///   [`OfferArgError::Unavailable`] with its reason.
    pub fn press(&self, args: &OfferArgs) -> Result<UiAction, OfferArgError> {
        let action = self.press_unchecked(args)?;
        match &action.meta().enablement {
            ActionEnablement::Enabled => Ok(action),
            ActionEnablement::Disabled { reason } => Err(OfferArgError::Unavailable {
                reason: reason.clone(),
            }),
        }
    }

    /// [`Self::press`] without the final enablement check.
    fn press_unchecked(&self, args: &OfferArgs) -> Result<UiAction, OfferArgError> {
        let known = || self.params.iter().map(|param| param.name.clone()).collect();
        if let Some((name, _)) = args
            .iter()
            .find(|(name, _)| !self.params.iter().any(|param| param.name == *name))
        {
            return Err(OfferArgError::Unknown {
                name: name.to_string(),
                known: known(),
            });
        }
        let Some(binder) = &self.binder else {
            return Ok(self.action.clone());
        };
        let mut resolved = OfferArgs::new();
        for param in &self.params {
            match args.get(&param.name) {
                Some(value) => {
                    param.check(value)?;
                    resolved.insert(param.name.clone(), value);
                }
                None => match param.default_value() {
                    Some(value) => resolved.insert(param.name.clone(), value),
                    None if param.is_required() => {
                        return Err(OfferArgError::Missing {
                            name: param.name.clone(),
                            label: param.label.clone(),
                        });
                    }
                    None => {}
                },
            }
        }
        self.check_widened_choices(&resolved)?;
        binder.bind(&resolved)
    }

    /// Refuse a choice option that is offered only with a toggle on
    /// ([`crate::OfferChoice::only_with`]) when the resolved press leaves
    /// that toggle off.
    fn check_widened_choices(&self, resolved: &OfferArgs) -> Result<(), OfferArgError> {
        for param in &self.params {
            let OfferParamKind::Choice { options, .. } = &param.kind else {
                continue;
            };
            let Some(value) = resolved.choice(&param.name) else {
                continue;
            };
            let Some(toggle) = options
                .iter()
                .find(|option| option.value == value)
                .and_then(|option| option.only_with.as_deref())
            else {
                continue;
            };
            if resolved.toggle(toggle) != Some(true) {
                return Err(OfferArgError::OptionDisabled {
                    name: param.name.clone(),
                    value: value.to_string(),
                    reason: format!("it is offered only with `{toggle}` on"),
                });
            }
        }
        Ok(())
    }

    /// Visible label (tooltip and accessible name).
    pub fn label(&self) -> &str {
        &self.action.meta().label
    }

    /// Help text or tooltip copy.
    pub fn summary(&self) -> &str {
        &self.action.meta().summary
    }

    /// True when the action carries primary emphasis.
    pub fn is_primary(&self) -> bool {
        self.action.meta().priority == ActionPriority::Primary
    }

    /// True when the action can currently be invoked.
    pub fn is_enabled(&self) -> bool {
        self.action.meta().enablement.is_enabled()
    }

    /// What pressing it costs the user.
    pub fn consequence(&self) -> &ActionConsequence {
        &self.action.meta().consequence
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ControllerId, OfferArgError, OfferArgs, OfferBinder, OfferChoice, OfferParam, OfferPath,
        ProjectOp, UiAction, UiOffer,
    };

    #[test]
    fn offer_exposes_wrapped_action_metadata() {
        let offer = UiOffer::new(
            OfferPath::project().child("save"),
            "save",
            UiAction::from_op(ControllerId::new("studio|project"), ProjectOp::SaveOverlay),
        );

        assert_eq!(offer.icon, "save");
        assert_eq!(offer.path.to_string(), "project/save");
        assert_eq!(offer.label(), "Save");
        assert!(offer.is_primary());
        assert!(offer.is_enabled());
        assert!(offer.consequence().is_routine());
    }

    #[test]
    fn a_parameterless_offer_ignores_empty_args_and_refuses_others() {
        let offer = save_offer();
        assert_eq!(offer.press(&OfferArgs::new()), Ok(offer.action.clone()));
        assert_eq!(
            offer.press(&OfferArgs::new().with("board", "xiao")),
            Err(OfferArgError::Unknown {
                name: "board".to_string(),
                known: Vec::new(),
            })
        );
        assert!(offer.params().is_empty());

        let disabled = UiOffer::new(
            OfferPath::project().child("save"),
            "save",
            offer.action.clone().disabled("nothing to save"),
        );
        assert_eq!(
            disabled.press(&OfferArgs::new()),
            Err(OfferArgError::Unavailable {
                reason: "nothing to save".to_string()
            })
        );
    }

    #[test]
    fn a_parameterised_offer_validates_fills_defaults_and_binds() {
        let offer = labelled_offer(Some("xiao"));
        assert!(offer.is_enabled(), "the preselect binds the default action");
        assert_eq!(offer.label(), "Save as xiao [loud]");
        assert_eq!(offer.params().len(), 3);

        let pressed = offer
            .press(&OfferArgs::new().with("note", "  hi "))
            .expect("defaults fill the choice and the toggle");
        assert_eq!(pressed.meta().label, "Save as xiao (hi) [loud]");

        let pressed = offer
            .press(
                &OfferArgs::new()
                    .with("board", "devkit")
                    .with("loud", "false"),
            )
            .unwrap();
        assert_eq!(pressed.meta().label, "Save as devkit");

        for (args, expected) in [
            (
                OfferArgs::new().with("board", "nope"),
                "`board` must be one of xiao, devkit, old; `nope` is not one of them",
            ),
            (
                OfferArgs::new().with("board", "old"),
                "`board` cannot be `old` right now: retired",
            ),
            (
                OfferArgs::new().with("note", "far too long"),
                "`note` is longer than 4 characters",
            ),
            (
                OfferArgs::new().with("loud", "maybe"),
                "`loud` takes true or false, not `maybe`",
            ),
            (
                OfferArgs::new().with("colour", "red"),
                "this offer has no parameter `colour`; it takes board, note, loud",
            ),
        ] {
            assert_eq!(
                offer.press(&args).map_err(|error| error.to_string()),
                Err(expected.to_string())
            );
        }
    }

    #[test]
    fn with_nothing_preselected_the_offer_reads_disabled_but_still_binds() {
        let offer = labelled_offer(None);
        assert!(!offer.is_enabled());
        assert_eq!(
            offer.action.meta().enablement,
            crate::ActionEnablement::Disabled {
                reason: "choose a board".to_string()
            }
        );
        assert_eq!(offer.label(), "Save", "the unbound action's own words");
        assert_eq!(
            offer.press(&OfferArgs::new()),
            Err(OfferArgError::Missing {
                name: "board".to_string(),
                label: "board".to_string()
            })
        );
        assert!(offer.press(&OfferArgs::new().with("board", "xiao")).is_ok());
    }

    #[test]
    fn an_option_widened_by_a_toggle_needs_the_toggle_on() {
        let params = vec![
            OfferParam::choice(
                "board",
                "board",
                vec![
                    OfferChoice::new("xiao", "XIAO"),
                    OfferChoice::new("s3", "S3 DevKit").only_with("all_boards"),
                ],
                Some("xiao".to_string()),
            ),
            OfferParam::toggle("all_boards", "every served board", false),
        ];
        let binder = OfferBinder::new(|args: &OfferArgs| {
            Ok(save_offer()
                .action
                .with_label(format!("Save as {}", args.choice("board").unwrap_or("?"))))
        });
        let offer = UiOffer::with_params(
            OfferPath::project().child("save-as"),
            "save",
            params,
            binder,
            save_offer().action,
        );

        assert!(offer.is_enabled(), "the narrowed preselect binds");
        assert_eq!(
            offer
                .press(&OfferArgs::new().with("board", "s3"))
                .map_err(|error| error.to_string()),
            Err(
                "`board` cannot be `s3` right now: it is offered only with `all_boards` on"
                    .to_string()
            )
        );
        let widened = offer
            .press(
                &OfferArgs::new()
                    .with("board", "s3")
                    .with("all_boards", "true"),
            )
            .expect("the toggle widens the choice");
        assert_eq!(widened.meta().label, "Save as s3");
    }

    #[test]
    fn equality_never_compares_the_binder() {
        assert_eq!(labelled_offer(Some("xiao")), labelled_offer(Some("xiao")));
        assert_ne!(labelled_offer(Some("xiao")), labelled_offer(None));
    }

    fn save_offer() -> UiOffer {
        UiOffer::new(
            OfferPath::project().child("save"),
            "save",
            UiAction::from_op(ControllerId::new("studio|project"), ProjectOp::SaveOverlay),
        )
    }

    /// A stand-in parameterised offer: its binder spells what it was given
    /// into the bound action's label, so a test can read the binding back.
    fn labelled_offer(preselect: Option<&str>) -> UiOffer {
        let params = vec![
            OfferParam::choice(
                "board",
                "board",
                vec![
                    OfferChoice::new("xiao", "XIAO"),
                    OfferChoice::new("devkit", "DevKit"),
                    OfferChoice::new("old", "Old").disabled("retired"),
                ],
                preselect.map(str::to_string),
            ),
            OfferParam::text("note", "note", "no note")
                .optional()
                .max_len(4),
            OfferParam::toggle("loud", "loudness", true),
        ];
        let binder = OfferBinder::new(|args: &OfferArgs| {
            let mut label = format!("Save as {}", args.choice("board").unwrap_or("?"));
            if let Some(note) = args.text("note") {
                label.push_str(&format!(" ({note})"));
            }
            if args.toggle("loud") == Some(true) {
                label.push_str(" [loud]");
            }
            Ok(save_offer().action.with_label(label))
        });
        UiOffer::with_params(
            OfferPath::project().child("save-as"),
            "save",
            params,
            binder,
            save_offer().action,
        )
    }

    #[test]
    fn offer_reflects_disabled_and_secondary_metadata() {
        let offer = UiOffer::new(
            OfferPath::project().child("revert"),
            "revert",
            UiAction::from_op(
                ControllerId::new("studio|project"),
                ProjectOp::RevertAllEdits,
            )
            .with_label("Revert to saved")
            .disabled("nothing to revert"),
        );

        assert_eq!(offer.label(), "Revert to saved");
        assert!(!offer.is_primary());
        assert!(!offer.is_enabled());
    }
}
