//! Daemon server — runs the HTTP/WS API server.
//!
//! Extracted from the original `agentd` main function so it can be called
//! from the `alan daemon start` subcommand.

use alan_agent_engine::{Config, LoadedConfig};
use anyhow::Result;
use axum::{
    Extension, Router, middleware,
    routing::{any, delete, get, post},
};
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::{info, warn};

use super::api_contract::paths;
use super::relay::{self, RelayClientConfig, RelayHub};
use super::remote_control::{RemoteAccessControl, remote_access_middleware};
use super::state::AppState;
use super::websocket;
use super::{connection_routes, routes};
use crate::host_config::HostConfig;

/// Run the daemon server with the given configuration.
///
/// This function blocks until the server is shut down.
#[allow(dead_code)]
pub async fn run_server(config: Config) -> Result<()> {
    run_server_with_loaded_config(LoadedConfig {
        config,
        path: None,
        source: alan_agent_engine::ConfigSourceKind::Default,
    })
    .await
}

pub async fn run_server_with_loaded_config(loaded_config: LoadedConfig) -> Result<()> {
    info!("Starting alan daemon");

    // Create app state
    let state = AppState::from_loaded_config(loaded_config);
    state.start_cleanup_task();
    state.start_scheduler_task();
    if let Err(err) = state.ensure_sessions_recovered().await {
        warn!(error = %err, "Failed to recover persisted sessions during daemon startup");
    }
    let remote_access = Arc::new(RemoteAccessControl::from_env()?);
    let relay_hub = RelayHub::from_env()?;
    info!(
        remote_auth_enabled = remote_access.enabled(),
        "Remote access control initialized"
    );
    info!(
        relay_server_enabled = relay_hub.enabled(),
        "Relay server configuration initialized"
    );

    if let Some(relay_client_config) = RelayClientConfig::from_env()? {
        relay::spawn_relay_client(relay_client_config);
        info!("Relay outbound tunnel client started");
    }

    let remote_access_layer = {
        let remote_access = Arc::clone(&remote_access);
        middleware::from_fn(move |request, next| {
            let remote_access = Arc::clone(&remote_access);
            async move { remote_access_middleware(remote_access, request, next).await }
        })
    };

    // Build router
    let mut app = Router::new()
        // Health check
        .route(paths::HEALTH, get(routes::health))
        .route(
            paths::CONNECTIONS_CATALOG,
            get(connection_routes::get_catalog),
        )
        .route(
            paths::CONNECTIONS,
            get(connection_routes::list_connections).post(connection_routes::create_connection),
        )
        .route(
            paths::CONNECTIONS_CURRENT,
            get(connection_routes::get_connection_current),
        )
        .route(
            paths::CONNECTIONS_DEFAULT_SET,
            post(connection_routes::set_connection_default),
        )
        .route(
            paths::CONNECTIONS_DEFAULT_CLEAR,
            post(connection_routes::clear_connection_default),
        )
        .route(
            paths::CONNECTIONS_PIN,
            post(connection_routes::pin_connection),
        )
        .route(
            paths::CONNECTIONS_UNPIN,
            post(connection_routes::unpin_connection),
        )
        .route(
            paths::CONNECTIONS_EVENTS,
            get(connection_routes::stream_connection_events),
        )
        .route(
            paths::CONNECTIONS_EVENTS_READ,
            get(connection_routes::read_connection_events),
        )
        .route(
            paths::CONNECTION,
            get(connection_routes::get_connection)
                .patch(connection_routes::update_connection)
                .delete(connection_routes::delete_connection),
        )
        .route(
            paths::CONNECTION_ACTIVATE,
            post(connection_routes::activate_connection),
        )
        .route(
            paths::CONNECTION_CREDENTIAL_STATUS,
            get(connection_routes::get_connection_credential_status),
        )
        .route(
            paths::CONNECTION_CREDENTIAL_SECRET,
            post(connection_routes::post_connection_secret),
        )
        .route(
            paths::CONNECTION_BROWSER_LOGIN_START,
            post(connection_routes::start_connection_browser_login),
        )
        .route(
            paths::CONNECTION_DEVICE_LOGIN_START,
            post(connection_routes::start_connection_device_login),
        )
        .route(
            paths::CONNECTION_DEVICE_LOGIN_COMPLETE,
            post(connection_routes::complete_connection_device_login),
        )
        .route(
            paths::CONNECTION_CREDENTIAL_LOGOUT,
            post(connection_routes::logout_connection_credential),
        )
        .route(
            paths::CONNECTION_TEST,
            post(connection_routes::test_connection),
        )
        // API routes
        .route(
            paths::SESSIONS,
            post(routes::create_session).get(routes::list_sessions),
        )
        .route(paths::SKILLS_CATALOG, get(routes::get_skill_catalog))
        .route(
            paths::SKILLS_CHANGED,
            get(routes::get_skill_catalog_changed),
        )
        .route(
            paths::SKILLS_OVERRIDES,
            post(routes::write_skill_override_route),
        )
        .route(paths::SESSION, get(routes::get_session))
        .route(paths::SESSION_CHILD_RUNS, get(routes::list_child_runs))
        .route(paths::SESSION_CHILD_RUN, get(routes::get_child_run))
        .route(
            paths::SESSION_CHILD_RUN_TERMINATE,
            post(routes::terminate_child_run),
        )
        .route(paths::SESSION_READ, get(routes::read_session))
        .route(
            paths::SESSION_RECONNECT_SNAPSHOT,
            get(routes::reconnect_snapshot),
        )
        .route(paths::SESSION_HISTORY, get(routes::get_session_history))
        .route(paths::SESSION_EVENTS_READ, get(routes::read_events))
        .route(paths::SESSION, delete(routes::delete_session))
        .route(paths::SESSION_RESUME, post(routes::resume_session))
        .route(paths::SESSION_FORK, post(routes::fork_session))
        .route(paths::SESSION_ROLLBACK, post(routes::rollback_session))
        .route(paths::SESSION_COMPACT, post(routes::compact_session))
        .route(
            paths::SESSION_SCHEDULE_AT,
            post(routes::schedule_session_at),
        )
        .route(
            paths::SESSION_SLEEP_UNTIL,
            post(routes::sleep_session_until),
        )
        .route(paths::SESSION_SUBMIT, post(routes::submit_operation_route))
        .route(paths::SESSION_EVENTS, get(routes::stream_events))
        .route(paths::SESSION_WS, get(websocket::ws_handler))
        .with_state(state);

    if relay_hub.enabled() {
        let relay_router = Router::new()
            .route(paths::RELAY_NODES, get(relay::relay_list_nodes_handler))
            .route(paths::RELAY_TUNNEL, get(relay::relay_tunnel_handler))
            .route(paths::RELAY_PROXY, any(relay::relay_proxy_handler))
            .layer(Extension(relay_hub.clone()));
        app = app.merge(relay_router);
    }

    app = app
        // Middleware
        .layer(remote_access_layer)
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    // Get bind address from env or use default
    let addr: SocketAddr = HostConfig::resolve_bind_address()?.parse()?;

    info!(%addr, "Server listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
