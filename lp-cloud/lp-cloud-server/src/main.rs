//! The cloud service process.
//!
//! Reads its whole configuration from the environment (see
//! [`ServerConfig`](lp_cloud_server::ServerConfig)), opens the backends it
//! names, and serves the three planes until SIGTERM.

use std::net::SocketAddr;
use std::process::ExitCode;

use lp_cloud_server::{AppState, ServerConfig, build_router};

#[tokio::main]
async fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config = match ServerConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            log::error!("configuration: {error}");
            return ExitCode::FAILURE;
        }
    };
    let address = SocketAddr::new(config.bind, config.port);
    let base_url = config.base_url.clone();
    if config.dev_auth {
        log::warn!("dev auth is ON: {base_url}/auth/dev?email=you@example.com signs anyone in");
    }

    // Opening the stores blocks — SQLite touches a file, and the S3 adapter
    // builds its own runtime — so it happens off the executor like every
    // other store call in this crate.
    let state = tokio::task::spawn_blocking(move || AppState::from_config(config))
        .await
        .expect("opening the stores");

    let listener = match tokio::net::TcpListener::bind(address).await {
        Ok(listener) => listener,
        Err(error) => {
            log::error!("could not bind {address}: {error}");
            return ExitCode::FAILURE;
        }
    };
    log::info!("lp-cloud-server listening on http://{address}/ (base url {base_url})");

    if let Err(error) = axum::serve(listener, build_router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        log::error!("server stopped: {error}");
        return ExitCode::FAILURE;
    }
    log::info!("shut down cleanly");
    ExitCode::SUCCESS
}

/// Resolve when the platform asks us to stop.
///
/// SIGTERM is the one that matters: a fly.io deploy rolls the machine by
/// sending it, and a process that ignores it is killed mid-request a few
/// seconds later. Ctrl-C is here so a local run stops the same way it would
/// in production rather than by a signal nobody tested.
async fn shutdown_signal() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(error) => log::error!("could not listen for SIGTERM: {error}"),
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = interrupt => log::info!("interrupted — draining"),
        () = terminate => log::info!("SIGTERM — draining"),
    }
}
