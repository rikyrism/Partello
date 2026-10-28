//! Structured logging via `tracing`. `RUST_LOG` controls verbosity,
//! `LOG_FORMAT=json` switches to JSON lines for log aggregation.

use tracing_subscriber::{EnvFilter, fmt, prelude::*};

pub fn init(json: bool) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("partello=info,partello_server=info,tower_http=info,sqlx=warn"));
    let registry = tracing_subscriber::registry().with(filter);
    if json {
        registry.with(fmt::layer().json().flatten_event(true)).init();
    } else {
        registry.with(fmt::layer().compact()).init();
    }
}
