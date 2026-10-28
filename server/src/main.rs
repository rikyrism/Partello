use std::net::SocketAddr;

use partello::{build_state, config::Config, database::Store, observability, routes, spawn_maintenance};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::from_env()?;
    if std::env::args().any(|a| a == "--healthcheck") {
        return healthcheck(config.bind_addr.port()).await;
    }
    observability::init(config.log_json);
    tracing::info!(config = %config.redacted_summary(), "starting partello");

    let store = match &config.database_url {
        Some(url) => Store::connect(url).await?,
        None => {
            tracing::warn!("DATABASE_URL not set: using the in-memory store, data is lost on restart");
            Store::memory()
        }
    };

    let bind = config.bind_addr;
    let app = build_state(config, store);
    spawn_maintenance(&app);

    let listener = tokio::net::TcpListener::bind(bind).await?;
    tracing::info!(addr = %bind, "listening");
    axum::serve(listener, routes::router(app).into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("shut down cleanly");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}

/// Container health probe without curl in the image: GET /healthz on loopback.
async fn healthcheck(port: u16) -> anyhow::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port)).await?;
    stream
        .write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await?;
    let mut buf = [0u8; 16];
    stream.read_exact(&mut buf[..12]).await?;
    anyhow::ensure!(buf.starts_with(b"HTTP/1.1 200"), "unhealthy");
    Ok(())
}
