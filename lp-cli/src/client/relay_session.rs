//! The signed-in lightplayer.app session a `relay:` client and
//! `serve --relay` use, and the account key it fetches.
//!
//! The session is the raw value of the `lp_session` cookie, read from
//! `LP_CLOUD_SESSION` — never argv (shell history and `ps` keep argv), never
//! printed. Copy it out of a signed-in browser's cookies, or mint one with a
//! local server's dev login.
//!
//! With a session, the account's key (`GetAccountAccess`) is fetched once:
//! a `relay:` client tries it first, so a board the account plugged in by
//! USB opens at its tier with no password; `serve --relay` installs it in
//! the host board's store, so the host board registers under the account.

use anyhow::{Context, Result, bail};
use lpa_client::transport_relay::RELAY_SESSION_COOKIE;
use lpc_access::{derive_login_key, link_psk};
use lpc_cloud_api::request::{GetAccountAccess, GetMe};
use lpc_cloud_api::{
    AccountAccessInfo, CLOUD_API_VERSION, CloudCall, CloudCallSpec, CloudError, CloudReply,
    CloudRequest, MeInfo,
};
use lpc_wire::lp_link::secure_channel::{KeyId, Psk};

/// The environment variable a lightplayer.app session comes from.
pub const CLOUD_SESSION_ENV: &str = "LP_CLOUD_SESSION";

/// The session from [`CLOUD_SESSION_ENV`], if set and not empty.
pub fn cloud_session_from_env() -> Option<String> {
    std::env::var(CLOUD_SESSION_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// The signed-in account's key as a link key (key id = its salt, PSK =
/// `link_psk(K)`, `K` as Studio installs it), or nothing for a guest
/// session (guests hold no account key: they reach a board by its
/// password).
pub async fn account_link_keys(origin: &str, session: &str) -> Result<Vec<(KeyId, Psk)>> {
    match call(origin, session, GetAccountAccess).await? {
        Ok(access) => {
            let k = account_device_key(&access);
            Ok(vec![(KeyId(access.key_salt), Psk::new(link_psk(&k)))])
        }
        Err(CloudError::NotAuthenticated) => Ok(Vec::new()),
        Err(other) => bail!("{origin} refused the account key: {other:?}"),
    }
}

/// The signed-in account's key and the name its device entries are
/// labelled with ("<given name>'s account", as Studio labels them).
pub async fn account_access(origin: &str, session: &str) -> Result<(AccountAccessInfo, String)> {
    let access = match call(origin, session, GetAccountAccess).await? {
        Ok(access) => access,
        Err(CloudError::NotAuthenticated) => bail!(
            "{CLOUD_SESSION_ENV} is not a signed-in account's session (a guest has no account key)"
        ),
        Err(other) => bail!("{origin} refused the account key: {other:?}"),
    };
    let name = match call(origin, session, GetMe).await? {
        Ok(MeInfo {
            given_name,
            display_name,
            ..
        }) => given_name.unwrap_or(display_name),
        Err(_) => "lightplayer".to_string(),
    };
    Ok((access, name))
}

/// `K` exactly as Studio installs an account key: PBKDF2 at one iteration.
pub fn account_device_key(access: &AccountAccessInfo) -> [u8; 32] {
    derive_login_key(&access.key_secret, &access.key_salt, 1)
}

/// One control-plane call to `origin` with the session cookie.
async fn call<R>(origin: &str, session: &str, request: R) -> Result<Result<R::Response, CloudError>>
where
    R: CloudCallSpec,
{
    let request: CloudRequest = request.into();
    let reply: CloudReply = reqwest::Client::new()
        .post(format!("{}/api", origin.trim_end_matches('/')))
        .header("cookie", format!("{RELAY_SESSION_COOKIE}={session}"))
        .json(&CloudCall {
            version: CLOUD_API_VERSION,
            request,
        })
        .send()
        .await
        .with_context(|| format!("could not reach {origin}"))?
        .error_for_status()
        .with_context(|| format!("{origin} answered with an error"))?
        .json()
        .await
        .with_context(|| format!("{origin} did not answer with a cloud reply"))?;
    match reply.result {
        Ok(response) => R::extract(response)
            .map(Ok)
            .ok_or_else(|| anyhow::anyhow!("{origin} answered a call with the wrong response")),
        Err(error) => Ok(Err(error)),
    }
}
