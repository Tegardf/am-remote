// Release builds have no console window; debug builds keep it for the logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod tray;

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::Level;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use am_remote_agent::api::{self, AppState};
use am_remote_agent::config::{self, ConfigStore};
use am_remote_agent::discovery;
use am_remote_agent::media::{MediaBackend, NullBackend, WindowsBackend};
use am_remote_agent::search::{self, ItunesSearch, SearchBackend};

fn main() -> Result<()> {
    // The agent's own messages (debug builds: in detail); the HTTP libraries only when something is wrong.
    let own = if cfg!(debug_assertions) {
        Level::DEBUG
    } else {
        Level::INFO
    };
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            Targets::new()
                .with_default(Level::WARN)
                .with_target("am_remote_agent", own),
        )
        .init();

    let store = Arc::new(ConfigStore::open(config::default_path()?)?);
    let port = store.port();
    let media: Arc<dyn MediaBackend> = match WindowsBackend::start() {
        Ok(backend) => Arc::new(backend),
        Err(e) => {
            // The API and pairing still work; media endpoints answer 500 until the agent restarts.
            tracing::error!("media backend unavailable: {e:#}");
            Arc::new(NullBackend::default())
        }
    };
    let storefront = store.storefront().unwrap_or_else(search::system_storefront);
    tracing::info!(%storefront, "searching the Apple Music catalog of this country");
    let search: Arc<dyn SearchBackend> = Arc::new(ItunesSearch::new(storefront)?);
    let state = AppState::new(store, media, search)?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("cannot start the async runtime")?;
    let listener = runtime
        .block_on(tokio::net::TcpListener::bind(("0.0.0.0", port)))
        .with_context(|| format!("cannot listen on port {port} (is another agent running?)"))?;
    tracing::info!(port, "API listening on /api/v1");
    let app = api::router(state.clone());
    let server = runtime.spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("server stopped: {e}");
        }
    });

    let advertisement = match discovery::advertise(port) {
        Ok(advertisement) => Some(advertisement),
        Err(e) => {
            // Manual IP entry in the app still works without discovery.
            tracing::warn!("discovery is off: {e:#}");
            None
        }
    };

    let result = tray::run(&state);

    if let Some(advertisement) = advertisement {
        advertisement.stop();
    }
    server.abort();
    runtime.shutdown_timeout(Duration::from_secs(2));
    result
}
