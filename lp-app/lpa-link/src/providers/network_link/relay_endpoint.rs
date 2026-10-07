//! The endpoint a board reached through lightplayer.app's relay is at:
//! `relay:<board>`, and the socket that reaches it.
//!
//! `<board>` is the board's MAC as twelve lowercase hex digits — the relay's
//! board id (`lpc_relay::RelayBoardId`) and Studio's `BoardKey`, one
//! spelling. The socket is the relay's browser leg on the page's own origin,
//! `wss://<host>/relay/board/<board>` (`ws://` on a plain-http dev origin),
//! and it carries bare lp-link frames, one per binary message — exactly what
//! a board serves on its LAN `/link`. So everything above the socket is the
//! LAN path's; only the endpoint, the address and the key policy differ
//! (see [`super::KeyWalk::held_only`]).

/// The endpoint scheme a relay link wears.
pub const RELAY_ENDPOINT_PREFIX: &str = "relay:";

/// The browser leg's path, before the board id.
pub const RELAY_SOCKET_PATH: &str = "/relay/board/";

/// The endpoint for the board `board` (twelve lowercase hex digits).
pub fn relay_endpoint(board: &str) -> String {
    format!("{RELAY_ENDPOINT_PREFIX}{board}")
}

/// The board a `relay:` endpoint names, or `None` for any other endpoint.
pub fn board_from_relay_endpoint(endpoint: &str) -> Option<&str> {
    endpoint
        .strip_prefix(RELAY_ENDPOINT_PREFIX)
        .filter(|board| is_board_id(board))
}

/// The browser leg for `board` on the relay at `origin`
/// (`https://lightplayer.app` → `wss://lightplayer.app/relay/board/<board>`;
/// `http://127.0.0.1:2812` → `ws://…`). An origin that is neither http nor
/// https is used as it is.
pub fn relay_socket_url(origin: &str, board: &str) -> String {
    let origin = origin.trim_end_matches('/');
    let socket_origin = if let Some(rest) = origin.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = origin.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        origin.to_string()
    };
    format!("{socket_origin}{RELAY_SOCKET_PATH}{board}")
}

/// The board a relay browser-leg URL dials, or `None` when `url` is not one
/// (a board's own LAN socket, `ws://<board>/link`, never is).
pub fn board_from_relay_socket_url(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("wss://")
        .or_else(|| url.strip_prefix("ws://"))?;
    let (_, path) = rest.split_once('/')?;
    let board = path.strip_prefix(&RELAY_SOCKET_PATH[1..])?;
    is_board_id(board).then_some(board)
}

/// The host (and port) a relay socket URL is on: `lightplayer.app`.
pub fn relay_host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split(['/', '?', '#']).next().unwrap_or(rest)
}

/// Twelve lowercase hex digits: a board id as Studio and the relay spell it.
fn is_board_id(text: &str) -> bool {
    text.len() == 12
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOARD: &str = "a0f26287b48c";

    #[test]
    fn a_relay_endpoint_round_trips_its_board() {
        let endpoint = relay_endpoint(BOARD);
        assert_eq!(endpoint, "relay:a0f26287b48c");
        assert_eq!(board_from_relay_endpoint(&endpoint), Some(BOARD));
    }

    #[test]
    fn other_endpoints_and_bad_ids_are_not_relay_ones() {
        for endpoint in [
            "lan:ws://10.0.0.5/link",
            "ble:QkxF",
            "relay:",
            "relay:A0F26287B48C",
            "relay:a0f26287b48",
            "relay:a0f26287b48cd",
            "relay:a0:f2:62:87:b4:8c",
        ] {
            assert_eq!(board_from_relay_endpoint(endpoint), None, "{endpoint}");
        }
    }

    #[test]
    fn the_socket_is_the_browser_leg_on_the_origins_socket_scheme() {
        assert_eq!(
            relay_socket_url("https://lightplayer.app", BOARD),
            "wss://lightplayer.app/relay/board/a0f26287b48c"
        );
        assert_eq!(
            relay_socket_url("http://127.0.0.1:2812/", BOARD),
            "ws://127.0.0.1:2812/relay/board/a0f26287b48c"
        );
    }

    #[test]
    fn a_relay_socket_names_its_board_and_a_lan_socket_does_not() {
        let url = relay_socket_url("https://lightplayer.app", BOARD);
        assert_eq!(board_from_relay_socket_url(&url), Some(BOARD));
        assert_eq!(relay_host(&url), "lightplayer.app");
        assert_eq!(
            board_from_relay_socket_url("ws://127.0.0.1:2812/relay/board/a0f26287b48c"),
            Some(BOARD)
        );
        for not_relay in [
            "ws://10.0.0.5/link",
            "ws://lp-b48c.local/link",
            "ws://10.0.0.5/relay/board/zzzz",
            "https://lightplayer.app/relay/board/a0f26287b48c",
            "ws://10.0.0.5/x/relay/board/a0f26287b48c",
        ] {
            assert_eq!(board_from_relay_socket_url(not_relay), None, "{not_relay}");
        }
    }
}
