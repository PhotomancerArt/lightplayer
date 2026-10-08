//! Which options of a choice are shown for a press's values — the one
//! reading a renderer draws from and [`UiOffer::press`] checks against, so
//! the list a person sees and what a press may pick cannot disagree.
//!
//! - An option offered [`OfferChoice::only_with`] a parameter is shown only
//!   while that parameter widens the list: a toggle that is on, or a text
//!   parameter holding text (the choice's own filter).
//! - A choice [`OfferParam::filter`]ed by a text parameter shows, while
//!   that text is not blank, only the options it
//!   [`OfferChoice::matches`]; blank, the list is the unfiltered one.
//! - A press that leaves a filtered choice out while its text names an
//!   option exactly ([`OfferChoice::is_named_by`]) picks that option; when
//!   the text names none, the choice stays out and the binder reads the
//!   text itself (an exact version the list does not hold yet).

use crate::{OfferArgs, OfferChoice, OfferParam, OfferParamKind, UiOffer};

/// What a filtered choice says when its text finds no option.
pub const FILTER_FINDS_NOTHING: &str = "Nothing in the list matches.";

impl UiOffer {
    /// The options of choice `param` shown for `args`, in their declared
    /// order. Empty for a parameter that is not a choice.
    pub fn shown_options<'a>(
        &self,
        param: &'a OfferParam,
        args: &OfferArgs,
    ) -> Vec<&'a OfferChoice> {
        let OfferParamKind::Choice { options, .. } = &param.kind else {
            return Vec::new();
        };
        let query = self.filter_text(param, args);
        options
            .iter()
            .filter(|option| match &option.only_with {
                Some(widener) => self.widens(widener, args),
                None => true,
            })
            .filter(|option| query.is_none_or(|query| option.matches(query)))
            .collect()
    }

    /// The filter text of choice `param` in `args`, trimmed, when it holds
    /// any.
    pub fn filter_text<'a>(&self, param: &OfferParam, args: &'a OfferArgs) -> Option<&'a str> {
        args.text(param.filter.as_deref()?)
    }

    /// A toggle's value: what `args` says, else its current state.
    pub fn toggle_value(&self, args: &OfferArgs, name: &str) -> bool {
        args.toggle(name).unwrap_or_else(|| {
            self.params()
                .iter()
                .find(|param| param.name == name)
                .and_then(|param| match param.kind {
                    OfferParamKind::Toggle { value } => Some(value),
                    _ => None,
                })
                .unwrap_or(false)
        })
    }

    /// `args` as a press should carry them: a picked option no longer shown
    /// (the list changed under it, its toggle was turned off, or the filter
    /// text stopped finding it) is dropped, so the choice's default stands
    /// in rather than a value the person no longer sees — the stale-pick
    /// guard.
    pub fn resolved_args(&self, args: &OfferArgs) -> OfferArgs {
        let mut resolved = OfferArgs::new();
        for (name, value) in args.iter() {
            let stale = self.params().iter().any(|param| {
                param.name == name
                    && matches!(param.kind, OfferParamKind::Choice { .. })
                    && !self
                        .shown_options(param, args)
                        .iter()
                        .any(|option| option.value == value)
            });
            if !stale {
                resolved.insert(name, value);
            }
        }
        resolved
    }

    /// The value a press that leaves choice `param` out gets: while its
    /// filter holds text, the shown, enabled option that text names
    /// exactly (else nothing); otherwise its preselect.
    pub fn choice_default(&self, param: &OfferParam, args: &OfferArgs) -> Option<String> {
        match self.filter_text(param, args) {
            Some(query) => self
                .shown_options(param, args)
                .into_iter()
                .find(|option| option.disabled.is_none() && option.is_named_by(query))
                .map(|option| option.value.clone()),
            None => param.default_value(),
        }
    }

    /// Whether `widener` widens a list for `args`: a toggle that is on, or
    /// a text parameter that holds text.
    fn widens(&self, widener: &str, args: &OfferArgs) -> bool {
        let text = self.params().iter().any(|param| {
            param.name == widener && matches!(param.kind, OfferParamKind::Text { .. })
        });
        match text {
            true => args.text(widener).is_some(),
            false => self.toggle_value(args, widener),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ControllerId, OfferArgError, OfferArgs, OfferBinder, OfferChoice, OfferParam, OfferPath,
        ProjectOp, UiAction, UiOffer,
    };

    #[test]
    fn blank_filter_text_shows_the_narrowed_list() {
        let offer = versions();
        let version = &offer.params()[1];
        let shown = values(offer.shown_options(version, &OfferArgs::new()));
        assert_eq!(shown, ["10.07-2", "10.07-1"]);
        let blank = OfferArgs::new().with("find", "   ");
        assert_eq!(values(offer.shown_options(version, &blank)), shown);
    }

    #[test]
    fn filter_text_searches_every_option_the_narrowed_ones_included() {
        let offer = versions();
        let version = &offer.params()[1];
        let args = OfferArgs::new().with("find", "10.0");
        assert_eq!(
            values(offer.shown_options(version, &args)),
            ["10.07-2", "10.07-1", "10.03-1"]
        );
        let args = OfferArgs::new().with("find", " 10.03 ");
        assert_eq!(values(offer.shown_options(version, &args)), ["10.03-1"]);
        let args = OfferArgs::new().with("find", "OLDER");
        assert_eq!(
            values(offer.shown_options(version, &args)),
            ["10.03-1"],
            "the detail is searched, ignoring case"
        );
    }

    #[test]
    fn typed_text_that_names_an_option_picks_it_and_any_other_text_leaves_the_binder_to_read_it() {
        let offer = versions();
        let pressed =
            |args: OfferArgs| offer.press(&args).map(|action| action.meta().label.clone());
        assert_eq!(pressed(OfferArgs::new()).as_deref(), Ok("Pick 10.07-2"));
        assert_eq!(
            pressed(OfferArgs::new().with("find", "10.03-1")).as_deref(),
            Ok("Pick 10.03-1"),
            "an exact name picks the option behind the filter"
        );
        assert_eq!(
            pressed(OfferArgs::new().with("find", "10.0")).as_deref(),
            Ok("Typed 10.0"),
            "a partial name picks nothing: the binder reads the text"
        );
    }

    #[test]
    fn a_pick_the_filter_does_not_show_is_refused_and_dropped() {
        let offer = versions();
        let hidden = OfferArgs::new().with("version", "10.03-1");
        assert!(matches!(
            offer.press(&hidden),
            Err(OfferArgError::OptionDisabled { reason, .. }) if reason.contains("`find`")
        ));
        let found = hidden.clone().with("find", "10.03");
        assert!(offer.press(&found).is_ok());
        let elsewhere = OfferArgs::new()
            .with("version", "10.07-1")
            .with("find", "10.03");
        assert!(offer.press(&elsewhere).is_err(), "typed away from the pick");
        assert_eq!(offer.resolved_args(&elsewhere).choice("version"), None);
        assert_eq!(
            offer.resolved_args(&found).choice("version"),
            Some("10.03-1")
        );
    }

    /// A version list: two recent, one only the box finds.
    fn versions() -> UiOffer {
        let pick = |label: String| {
            UiAction::from_op(ControllerId::new("studio|project"), ProjectOp::SaveOverlay)
                .with_label(label)
        };
        UiOffer::with_params(
            OfferPath::project().child("pick"),
            "save",
            vec![
                OfferParam::text("find", "find a version", "a version").optional(),
                OfferParam::choice(
                    "version",
                    "version",
                    vec![
                        OfferChoice::new("10.07-2", "10.07-2"),
                        OfferChoice::new("10.07-1", "10.07-1"),
                        OfferChoice::new("10.03-1", "10.03-1")
                            .with_detail("older language")
                            .only_with("find"),
                    ],
                    Some("10.07-2".to_string()),
                )
                .filtered_by("find"),
            ],
            OfferBinder::new(move |args: &OfferArgs| match args.choice("version") {
                Some(version) => Ok(pick(format!("Pick {version}"))),
                None => Ok(pick(format!("Typed {}", args.text("find").unwrap_or("")))),
            }),
            pick("Pick".to_string()),
        )
    }

    fn values(options: Vec<&OfferChoice>) -> Vec<&str> {
        options.iter().map(|option| option.value.as_str()).collect()
    }
}
