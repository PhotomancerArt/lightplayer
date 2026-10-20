//! [`row_switch`]: the switch a radio row's details lead with — Wi‑Fi,
//! Bluetooth and lightplayer.app build theirs here, so the three read and
//! press alike ([`UiDetailSwitch`]).

use super::board_card_input::BoardCardInput;
use super::ui_card_action::UiCardAction;
use super::ui_detail_switch::UiDetailSwitch;
use super::ui_stack_bar::BarLayer;
use crate::OfferArgs;

/// What one row's switch presses: the board's switch offer under the
/// board's path, its one toggle parameter, and the verbs a flip says.
pub(crate) struct SwitchOffer<'a> {
    /// The offer's path under the board: `["wifi", "enabled"]`.
    pub under: &'a [&'a str],
    /// Its toggle parameter.
    pub param: &'a str,
    /// The flip's word while it is on ("Disable Wi‑Fi").
    pub turn_off: &'a str,
    /// The flip's word while it is off ("Enable Wi‑Fi").
    pub turn_on: &'a str,
}

/// `layer`'s switch, drawn with `icon` and standing at `on`: flipped by
/// `offer` pressed to `!on`, or dimmed where this link is not offered it.
pub(crate) fn row_switch(
    input: &BoardCardInput<'_>,
    layer: BarLayer,
    icon: &str,
    on: bool,
    offer: SwitchOffer<'_>,
) -> UiDetailSwitch {
    let flip = input.offer_under(offer.under).map(|switch| {
        UiCardAction::press_with(
            switch,
            if on { offer.turn_off } else { offer.turn_on },
            OfferArgs::new().with(offer.param, (!on).to_string()),
        )
        .with_icon(icon)
    });
    UiDetailSwitch::new(layer.kind_word().unwrap_or_default(), icon, on, flip)
}
