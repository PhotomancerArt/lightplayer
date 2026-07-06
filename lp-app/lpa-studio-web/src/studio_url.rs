//! URL launch intent for the Studio web shell.
//!
//! The URL records browser launch/session intent such as
//! `?connect=simulator`. It is deliberately owned by the web crate so the
//! headless Studio controller stays independent of browser routing.

use lpa_studio_core::{DeviceController, DeviceOp, LinkProviderKind, UiAction};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

const CONNECTION_PARAM: &str = "connect";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConnectionIntent {
    Simulator,
    Usb,
}

impl ConnectionIntent {
    fn from_provider_kind(kind: LinkProviderKind) -> Option<Self> {
        match kind {
            LinkProviderKind::BrowserWorker => Some(Self::Simulator),
            LinkProviderKind::BrowserSerialEsp32 => Some(Self::Usb),
            LinkProviderKind::Fake
            | LinkProviderKind::HostProcess
            | LinkProviderKind::HostSerialEsp32 => None,
        }
    }

    fn from_query_value(value: &str) -> Option<Self> {
        match value {
            "simulator" => Some(Self::Simulator),
            "usb" => Some(Self::Usb),
            _ => None,
        }
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn query_value(self) -> &'static str {
        match self {
            Self::Simulator => "simulator",
            Self::Usb => "usb",
        }
    }

    fn provider_kind(self) -> LinkProviderKind {
        match self {
            Self::Simulator => LinkProviderKind::BrowserWorker,
            Self::Usb => LinkProviderKind::BrowserSerialEsp32,
        }
    }

    fn should_auto_open(self) -> bool {
        matches!(self, Self::Simulator)
    }

    pub(crate) fn startup_action(self) -> Option<UiAction> {
        self.should_auto_open().then(|| {
            UiAction::from_op(
                DeviceController::NODE_ID,
                DeviceOp::OpenProvider {
                    provider_id: self.provider_kind(),
                },
            )
        })
    }
}

pub(crate) fn read_connection_intent() -> Option<ConnectionIntent> {
    current_search()
        .as_deref()
        .and_then(connection_intent_from_search)
}

pub(crate) fn update_for_action(action: &UiAction) {
    let Some(op) = action.op_as::<DeviceOp>() else {
        return;
    };

    match op {
        DeviceOp::OpenProvider { provider_id } => {
            if let Some(intent) = ConnectionIntent::from_provider_kind(*provider_id) {
                write_connection_intent(Some(intent));
            }
        }
        DeviceOp::DisconnectDevice => write_connection_intent(None),
        DeviceOp::OpenProviderForRecovery { .. }
        | DeviceOp::ConnectEndpoint { .. }
        | DeviceOp::ConnectLightPlayer
        | DeviceOp::DisconnectLightPlayer
        | DeviceOp::ResetDevice
        | DeviceOp::ProvisionFirmware
        | DeviceOp::ResetToBlank
        | DeviceOp::RefreshConnections
        | DeviceOp::SetLogLevel { .. } => {}
    }
}

#[cfg(target_arch = "wasm32")]
fn current_search() -> Option<String> {
    web_sys::window()
        .map(|window| window.location())
        .and_then(|location| location.search().ok())
}

#[cfg(not(target_arch = "wasm32"))]
fn current_search() -> Option<String> {
    None
}

#[cfg(target_arch = "wasm32")]
fn write_connection_intent(intent: Option<ConnectionIntent>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let location = window.location();
    let pathname = location.pathname().unwrap_or_default();
    let search = location.search().unwrap_or_default();
    let hash = location.hash().unwrap_or_default();
    let next_url = format!(
        "{pathname}{}{hash}",
        search_with_connection_intent(&search, intent)
    );

    if let Ok(history) = window.history() {
        let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(&next_url));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn write_connection_intent(_intent: Option<ConnectionIntent>) {}

fn connection_intent_from_search(search: &str) -> Option<ConnectionIntent> {
    search
        .trim_start_matches('?')
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(key, value)| {
            (key == CONNECTION_PARAM)
                .then(|| ConnectionIntent::from_query_value(value))
                .flatten()
        })
}

#[cfg(any(target_arch = "wasm32", test))]
fn search_with_connection_intent(search: &str, intent: Option<ConnectionIntent>) -> String {
    let mut params = search
        .trim_start_matches('?')
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter(|pair| pair.split_once('=').map_or(*pair, |(key, _)| key) != CONNECTION_PARAM)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    if let Some(intent) = intent {
        params.push(format!("{CONNECTION_PARAM}={}", intent.query_value()));
    }

    if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    }
}

#[cfg(test)]
mod tests {
    use super::{ConnectionIntent, connection_intent_from_search, search_with_connection_intent};

    #[test]
    fn parses_connection_intent_from_search() {
        assert_eq!(
            connection_intent_from_search("?connect=simulator"),
            Some(ConnectionIntent::Simulator)
        );
        assert_eq!(
            connection_intent_from_search("?foo=1&connect=usb"),
            Some(ConnectionIntent::Usb)
        );
        assert_eq!(connection_intent_from_search("?connect=serial"), None);
    }

    #[test]
    fn writes_connection_intent_without_dropping_other_params() {
        assert_eq!(
            search_with_connection_intent("?foo=1", Some(ConnectionIntent::Simulator)),
            "?foo=1&connect=simulator"
        );
        assert_eq!(
            search_with_connection_intent("?connect=usb&foo=1", Some(ConnectionIntent::Simulator)),
            "?foo=1&connect=simulator"
        );
        assert_eq!(search_with_connection_intent("?connect=usb", None), "");
    }

    #[test]
    fn only_simulator_auto_opens_from_url() {
        assert!(ConnectionIntent::Simulator.startup_action().is_some());
        assert!(ConnectionIntent::Usb.startup_action().is_none());
    }
}
