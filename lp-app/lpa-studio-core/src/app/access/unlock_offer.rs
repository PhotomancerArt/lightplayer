//! `devices/<board>/unlock`: "Unlock" on a board whose link holds nothing
//! (or only play), as an offer with typed parameters.
//!
//! | param | kind | |
//! |---|---|---|
//! | `password` | **secret** text, optional | the board's device password; left out, the press raises the sheet instead |
//! | `remember` | toggle, on | keep it on this browser for the next board that takes it |
//!
//! WHEN it is offered ([`device_unlock_offer`]) is the access controller's
//! reading of the board's link ([`UiUnlockOffer`]) while the board is linked
//! and idle; WHAT it is — its words, its level, how a press binds — is
//! decided here, once, for the controller, the card, the Unlock sheet and
//! the stories that draw it.
//!
//! A press with a password is [`UnlockOp::Submit`] (what the sheet's submit
//! does); a press with none is [`UnlockOp::Ask`] (what the card's button
//! does: raise the sheet). So an agent that presses it hands the sheet to
//! the person, and an offer that takes a secret is, as ever, only ever
//! pressed with the secret by the person.

use lpa_devices::DeviceId;

use super::ui_access_view::UiUnlockOffer;
use super::unlock_op::UnlockOp;
use super::unlock_password::UnlockPassword;
use crate::{DeviceEscape, DeviceView, OfferArgs, OfferBinder, OfferParam, OfferPath, UiOffer};

/// The offer's verb under a board's prefix.
pub const UNLOCK_VERB: &str = "unlock";

/// The password parameter (a secret).
pub const UNLOCK_PASSWORD_PARAM: &str = "password";

/// The remember parameter.
pub const UNLOCK_REMEMBER_PARAM: &str = "remember";

/// The unlock offer under `prefix` (`devices/<board>`) for the board `view`
/// shows, when it has one: its link says it needs unlocking (`unlock`), and
/// it is linked and idle — the card has always withdrawn Unlock while work
/// runs.
pub fn device_unlock_offer(
    prefix: &OfferPath,
    view: &DeviceView,
    unlock: Option<UiUnlockOffer>,
) -> Option<UiOffer> {
    let unlock = unlock?;
    let linked = view.escapes.contains(&DeviceEscape::Disconnect);
    let idle = view.activity.is_none();
    (linked && idle).then(|| unlock_offer(prefix, view.id, unlock))
}

/// The verb under `prefix` for `device`, worded for what `unlock` says is
/// missing: "Unlock" with nothing granted, "Unlock to edit" with play.
/// Routine: it changes nothing lasting.
pub fn unlock_offer(prefix: &OfferPath, device: DeviceId, unlock: UiUnlockOffer) -> UiOffer {
    let label = match unlock {
        UiUnlockOffer::Locked => "Unlock",
        UiUnlockOffer::PlayOnly => "Unlock to edit",
    };
    let password = OfferParam::text(UNLOCK_PASSWORD_PARAM, "password", "device password")
        .optional()
        .secret();
    let remember = OfferParam::toggle(UNLOCK_REMEMBER_PARAM, "remember on this device", true);
    let ask = UnlockOp::action_for(UnlockOp::Ask { device }).with_label(label);
    UiOffer::with_params(
        prefix.clone().child(UNLOCK_VERB),
        "lock",
        vec![password, remember],
        OfferBinder::new(move |args: &OfferArgs| {
            Ok(UnlockOp::action_for(bind(device, args)).with_label(label))
        }),
        ask,
    )
}

/// The op one press binds: a password, as typed (spaces count, so it is
/// read raw and never trimmed) is a submit; none is the sheet.
fn bind(device: DeviceId, args: &OfferArgs) -> UnlockOp {
    match args
        .get(UNLOCK_PASSWORD_PARAM)
        .filter(|password| !password.is_empty())
    {
        Some(password) => UnlockOp::Submit {
            device,
            password: UnlockPassword::new(password),
            remember: args.toggle(UNLOCK_REMEMBER_PARAM).unwrap_or(true),
        },
        None => UnlockOp::Ask { device },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActionPriority, OfferParamKind};

    const PASSWORD: &str = "correct-horse-42";

    #[test]
    fn the_verb_lives_under_the_board_and_is_routine() {
        let offer = unlock_offer(&prefix(), DeviceId(7), UiUnlockOffer::Locked);
        assert_eq!(offer.path.to_string(), "devices/mac-a0f26287b48e/unlock");
        assert_eq!(offer.icon, "lock");
        assert!(offer.is_enabled());
        assert!(offer.consequence().is_routine());
        assert_eq!(offer.summary(), "Enter this board's password.");
        assert_eq!(offer.action.meta().priority, ActionPriority::Primary);
    }

    #[test]
    fn the_words_follow_what_is_missing() {
        let locked = unlock_offer(&prefix(), DeviceId(7), UiUnlockOffer::Locked);
        assert_eq!(locked.label(), "Unlock");
        let play = unlock_offer(&prefix(), DeviceId(7), UiUnlockOffer::PlayOnly);
        assert_eq!(play.label(), "Unlock to edit");
        // The label survives binding either way.
        let pressed = play
            .press(&OfferArgs::new().with(UNLOCK_PASSWORD_PARAM, PASSWORD))
            .unwrap();
        assert_eq!(pressed.meta().label, "Unlock to edit");
    }

    #[test]
    fn it_takes_a_secret_password_and_a_remember_that_starts_on() {
        let offer = unlock_offer(&prefix(), DeviceId(7), UiUnlockOffer::Locked);
        assert!(offer.takes_a_secret());
        let names: Vec<&str> = offer.params().iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, [UNLOCK_PASSWORD_PARAM, UNLOCK_REMEMBER_PARAM]);
        assert!(offer.params()[0].is_secret());
        assert!(matches!(
            offer.params()[0].kind,
            OfferParamKind::Text {
                optional: true,
                secret: true,
                ..
            }
        ));
        assert!(matches!(
            offer.params()[1].kind,
            OfferParamKind::Toggle { value: true }
        ));
    }

    #[test]
    fn a_press_with_a_password_submits_both_values() {
        let offer = unlock_offer(&prefix(), DeviceId(7), UiUnlockOffer::Locked);
        let pressed = offer
            .press(
                &OfferArgs::new()
                    .with(UNLOCK_PASSWORD_PARAM, PASSWORD)
                    .with(UNLOCK_REMEMBER_PARAM, "false"),
            )
            .unwrap();
        assert_eq!(
            pressed.op_as::<UnlockOp>().unwrap(),
            &UnlockOp::Submit {
                device: DeviceId(7),
                password: UnlockPassword::new(PASSWORD),
                remember: false,
            }
        );
        // Nothing the agent, a log or the recorder reads holds it.
        assert!(!format!("{pressed:?}").contains(PASSWORD));
        assert_eq!(
            pressed
                .offer_press()
                .unwrap()
                .args
                .get(UNLOCK_PASSWORD_PARAM),
            Some(crate::SECRET_MARKER)
        );
    }

    #[test]
    fn remember_defaults_on() {
        let offer = unlock_offer(&prefix(), DeviceId(7), UiUnlockOffer::Locked);
        let pressed = offer
            .press(&OfferArgs::new().with(UNLOCK_PASSWORD_PARAM, PASSWORD))
            .unwrap();
        assert!(matches!(
            pressed.op_as::<UnlockOp>().unwrap(),
            UnlockOp::Submit { remember: true, .. }
        ));
    }

    #[test]
    fn a_password_is_taken_as_typed() {
        let offer = unlock_offer(&prefix(), DeviceId(7), UiUnlockOffer::Locked);
        let pressed = offer
            .press(&OfferArgs::new().with(UNLOCK_PASSWORD_PARAM, " two words "))
            .unwrap();
        match pressed.op_as::<UnlockOp>().unwrap() {
            UnlockOp::Submit { password, .. } => {
                assert_eq!(password.clone().into_string(), " two words ");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_press_with_no_password_raises_the_sheet() {
        let offer = unlock_offer(&prefix(), DeviceId(7), UiUnlockOffer::PlayOnly);
        let ask = UnlockOp::Ask {
            device: DeviceId(7),
        };
        // Bare, with the remember switch alone, and with a blank password.
        for args in [
            OfferArgs::new(),
            OfferArgs::new().with(UNLOCK_REMEMBER_PARAM, "false"),
            OfferArgs::new().with(UNLOCK_PASSWORD_PARAM, ""),
        ] {
            let pressed = offer.press(&args).unwrap();
            assert_eq!(pressed.op_as::<UnlockOp>().unwrap(), &ask);
        }
        // The unbound action is the same form: a plain press of the card's
        // button raises the sheet.
        assert_eq!(offer.action.op_as::<UnlockOp>().unwrap(), &ask);
        assert!(offer.is_enabled());
    }

    #[test]
    fn a_name_it_does_not_take_is_refused() {
        let offer = unlock_offer(&prefix(), DeviceId(7), UiUnlockOffer::Locked);
        assert!(offer.press(&OfferArgs::new().with("tier", "edit")).is_err());
    }

    fn prefix() -> OfferPath {
        OfferPath::devices().child("mac-a0f26287b48e")
    }
}
