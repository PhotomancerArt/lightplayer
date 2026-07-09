//! URL launch intent for the Studio web shell.
//!
//! The URL records browser launch/session intent: `?connect=simulator` (the
//! pre-gallery auto-open) and `?project=prj_…` (the open library project, so
//! a reload re-opens it — load-as-push makes that a fresh push of the
//! library head). It is deliberately owned by the web crate so the headless
//! Studio controller stays independent of browser routing. Home is the
//! default route: no params, no auto-open.

use lpa_studio_core::{
    DeviceController, DeviceOp, HOME_NODE_ID, HomeOp, LinkProviderKind, ProjectController,
    ProjectOp, UiAction,
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;

const CONNECTION_PARAM: &str = "connect";
const PROJECT_PARAM: &str = "project";

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
}

/// The action the URL asks the shell to run at startup, if any: an open of
/// the recorded project wins over the legacy simulator auto-open.
pub(crate) fn read_startup_action() -> Option<UiAction> {
    let search = current_search();
    let search = search.as_deref().unwrap_or("");
    startup_action_from_search(search)
}

fn startup_action_from_search(search: &str) -> Option<UiAction> {
    if let Some(uid) = project_from_search(search) {
        return Some(UiAction::from_op(HOME_NODE_ID, HomeOp::OpenPackage { uid }));
    }
    connection_intent_from_search(search)
        .filter(|intent| intent.should_auto_open())
        .map(|intent| {
            UiAction::from_op(
                DeviceController::NODE_ID,
                DeviceOp::OpenProvider {
                    provider_id: intent.provider_kind(),
                },
            )
        })
}

/// Mirror a dispatched action into the URL, so a reload lands back where
/// the user is.
pub(crate) fn update_for_action(action: &UiAction) {
    if action.node_id().as_str() == HOME_NODE_ID {
        match action.op_as::<HomeOp>() {
            Some(HomeOp::OpenPackage { uid }) => write_param(PROJECT_PARAM, Some(uid)),
            // an example's library uid is only known after the open; the
            // param updates on the next explicit open of the copy
            Some(HomeOp::OpenExample { .. }) | Some(HomeOp::DeletePackage { .. }) => {
                write_param(PROJECT_PARAM, None);
            }
            _ => {}
        }
        return;
    }

    if action.node_id().as_str() == ProjectController::NODE_ID {
        if let Some(ProjectOp::DisconnectProject) = action.op_as::<ProjectOp>() {
            write_param(PROJECT_PARAM, None);
        }
        return;
    }

    let Some(op) = action.op_as::<DeviceOp>() else {
        return;
    };
    match op {
        DeviceOp::OpenProvider { provider_id } => {
            if let Some(intent) = ConnectionIntent::from_provider_kind(*provider_id) {
                write_param(CONNECTION_PARAM, Some(intent.query_value()));
            }
        }
        DeviceOp::DisconnectDevice => {
            write_param(CONNECTION_PARAM, None);
            write_param(PROJECT_PARAM, None);
        }
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
fn write_param(key: &str, value: Option<&str>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let location = window.location();
    let pathname = location.pathname().unwrap_or_default();
    let search = location.search().unwrap_or_default();
    let hash = location.hash().unwrap_or_default();
    let next_url = format!("{pathname}{}{hash}", search_with_param(&search, key, value));

    if let Ok(history) = window.history() {
        let _ = history.replace_state_with_url(&JsValue::NULL, "", Some(&next_url));
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn write_param(_key: &str, _value: Option<&str>) {}

fn param_from_search<'a>(search: &'a str, key: &str) -> Option<&'a str> {
    search
        .trim_start_matches('?')
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find_map(|(pair_key, value)| (pair_key == key).then_some(value))
}

fn connection_intent_from_search(search: &str) -> Option<ConnectionIntent> {
    param_from_search(search, CONNECTION_PARAM).and_then(ConnectionIntent::from_query_value)
}

fn project_from_search(search: &str) -> Option<String> {
    param_from_search(search, PROJECT_PARAM)
        .filter(|uid| uid.starts_with("prj_"))
        .map(str::to_string)
}

#[cfg(any(target_arch = "wasm32", test))]
fn search_with_param(search: &str, key: &str, value: Option<&str>) -> String {
    let mut params = search
        .trim_start_matches('?')
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter(|pair| pair.split_once('=').map_or(*pair, |(pair_key, _)| pair_key) != key)
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    if let Some(value) = value {
        params.push(format!("{key}={value}"));
    }

    if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn writes_params_without_dropping_other_params() {
        assert_eq!(
            search_with_param("?foo=1", CONNECTION_PARAM, Some("simulator")),
            "?foo=1&connect=simulator"
        );
        assert_eq!(
            search_with_param("?connect=usb&foo=1", CONNECTION_PARAM, Some("simulator")),
            "?foo=1&connect=simulator"
        );
        assert_eq!(
            search_with_param("?connect=usb", CONNECTION_PARAM, None),
            ""
        );
        assert_eq!(
            search_with_param("?connect=simulator", PROJECT_PARAM, Some("prj_abc")),
            "?connect=simulator&project=prj_abc"
        );
    }

    #[test]
    fn project_param_wins_startup_and_becomes_an_open_action() {
        let action = startup_action_from_search("?connect=simulator&project=prj_abc")
            .expect("project param produces a startup action");
        assert_eq!(action.node_id().as_str(), HOME_NODE_ID);
        assert_eq!(
            action.op_as::<HomeOp>(),
            Some(&HomeOp::OpenPackage {
                uid: "prj_abc".to_string()
            })
        );
    }

    #[test]
    fn malformed_project_param_is_ignored() {
        assert!(startup_action_from_search("?project=notauid").is_none());
    }

    #[test]
    fn only_simulator_auto_opens_from_url() {
        let simulator =
            startup_action_from_search("?connect=simulator").expect("simulator intent auto-opens");
        assert!(matches!(
            simulator.op_as::<DeviceOp>(),
            Some(DeviceOp::OpenProvider {
                provider_id: LinkProviderKind::BrowserWorker
            })
        ));
        assert!(startup_action_from_search("?connect=usb").is_none());
        assert!(startup_action_from_search("").is_none());
    }
}
