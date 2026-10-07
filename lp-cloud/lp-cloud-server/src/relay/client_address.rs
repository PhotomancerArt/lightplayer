//! The address a request reached the relay from.
//!
//! Behind fly.io, every request arrives from fly's proxy, and the real
//! client's address is the `Fly-Client-IP` header, which the proxy sets and
//! overwrites (a client cannot forge it through fly). Locally there is no
//! proxy: the socket's peer address is the client
//! (`into_make_service_with_connect_info` in `main`). A request with
//! neither — the router driven in-process by a test — has no address.
//!
//! Used for two things, neither of them a credential: `sameNetwork` in
//! `ListBoards` (the board's and the browser's addresses match), and the
//! per-address visitor limit.

use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;

/// The header fly's proxy puts the client's address in.
pub const FLY_CLIENT_IP: &str = "fly-client-ip";

/// The client's address, if the relay can tell it. An extractor that never
/// fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientAddress(pub Option<IpAddr>);

impl<S: Send + Sync> FromRequestParts<S> for ClientAddress {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let from_fly = parts
            .headers
            .get(FLY_CLIENT_IP)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<IpAddr>().ok());
        let from_peer = || {
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|ConnectInfo(peer)| peer.ip())
        };
        Ok(Self(from_fly.or_else(from_peer)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    async fn address_of(request: Request<()>) -> Option<IpAddr> {
        let (mut parts, ()) = request.into_parts();
        let ClientAddress(ip) = ClientAddress::from_request_parts(&mut parts, &())
            .await
            .unwrap();
        ip
    }

    #[tokio::test]
    async fn fly_client_ip_wins_over_the_peer() {
        let mut request = Request::builder()
            .header(FLY_CLIENT_IP, "203.0.113.7")
            .body(())
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo::<SocketAddr>("10.0.0.1:5000".parse().unwrap()));
        assert_eq!(
            address_of(request).await,
            Some("203.0.113.7".parse().unwrap())
        );
    }

    #[tokio::test]
    async fn the_peer_is_used_without_the_header_and_nothing_without_either() {
        let mut request = Request::builder().body(()).unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo::<SocketAddr>("127.0.0.1:5000".parse().unwrap()));
        assert_eq!(
            address_of(request).await,
            Some("127.0.0.1".parse().unwrap())
        );
        assert_eq!(address_of(Request::builder().body(()).unwrap()).await, None);
        let garbage = Request::builder()
            .header(FLY_CLIENT_IP, "not an address")
            .body(())
            .unwrap();
        assert_eq!(address_of(garbage).await, None);
    }
}
