//! Partello backend library. `main.rs` wires it to the environment; tests use
//! [`build_state`] + [`routes::router`] with the in-memory store.

pub mod config;
pub mod database;
pub mod error;
pub mod observability;
pub mod rate_limit;
pub mod realtime;
pub mod room;
pub mod routes;
pub mod security;
pub mod validation;
pub mod youtube;
pub mod youtube_search;

use std::{net::IpAddr, sync::Arc, time::Duration};

use uuid::Uuid;

use crate::{config::Config, database::Store, rate_limit::KeyedLimiter, realtime::hub::Hub, youtube_search::SearchClient};

pub struct Limiters {
    pub create_room: KeyedLimiter<IpAddr>,
    pub join: KeyedLimiter<IpAddr>,
    pub create_key: KeyedLimiter<IpAddr>,
    pub api_calls: KeyedLimiter<Uuid>,
    /// Per participant: searches cost YouTube quota.
    pub search: KeyedLimiter<Uuid>,
}

impl Default for Limiters {
    fn default() -> Self {
        Self {
            create_room: KeyedLimiter::new(10, Duration::from_secs(60)),
            join: KeyedLimiter::new(30, Duration::from_secs(60)),
            create_key: KeyedLimiter::new(5, Duration::from_secs(3600)),
            api_calls: KeyedLimiter::new(60, Duration::from_secs(60)),
            search: KeyedLimiter::new(15, Duration::from_secs(60)),
        }
    }
}

pub struct AppInner {
    pub config: Arc<Config>,
    pub hub: Arc<Hub>,
    pub search: SearchClient,
    pub limiters: Limiters,
}

pub type AppState = Arc<AppInner>;

pub fn build_state(config: Config, store: Store) -> AppState {
    let config = Arc::new(config);
    Arc::new(AppInner {
        hub: Arc::new(Hub::new(store, config.clone())),
        search: SearchClient::new(config.youtube_api_key.clone(), config.youtube_search_daily_budget),
        config,
        limiters: Limiters::default(),
    })
}

/// Background housekeeping: presence sweeps every 15s, retention purge hourly.
pub fn spawn_maintenance(app: &AppState) {
    let hub = app.hub.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(15));
        let mut n: u64 = 0;
        loop {
            tick.tick().await;
            hub.maintenance().await;
            if n % 240 == 0 {
                if let Err(err) = hub.purge_inactive_rooms().await {
                    tracing::warn!(error = %err, "room purge failed");
                }
            }
            n += 1;
        }
    });
}
