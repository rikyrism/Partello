//! HTTP routing.

pub mod developer;
pub mod health;
pub mod rooms;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    http::{HeaderName, HeaderValue, Method, header},
    routing::{get, patch, post},
};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

use crate::{AppState, realtime::ws::ws_handler};

const BODY_LIMIT: usize = 16 * 1024;
const OPENAPI: &str = include_str!("../../../docs/openapi.yaml");

/// Content Security Policy for the web app: the only third parties are the
/// YouTube player (script + iframe) and its thumbnail CDN. WebSocket origins
/// are listed explicitly because older Safari does not match `wss:` to `'self'`.
fn csp(app: &AppState) -> String {
    let sockets: Vec<String> = app
        .config
        .allowed_origins
        .iter()
        .filter_map(|o| {
            o.strip_prefix("https://")
                .map(|h| format!("wss://{h}"))
                .or_else(|| o.strip_prefix("http://").map(|h| format!("ws://{h}")))
        })
        .collect();
    // The LiveKit SFU, when calls go through one, is another socket origin.
    let sfu = app
        .config
        .livekit
        .as_ref()
        .and_then(|lk| url::Url::parse(&lk.url).ok())
        .map(|u| u.origin().ascii_serialization())
        .unwrap_or_default();
    let sfu_https = sfu.replacen("wss://", "https://", 1).replacen("ws://", "http://", 1);
    format!(
        "default-src 'self'; script-src 'self' https://www.youtube.com https://s.ytimg.com; \
         frame-src https://www.youtube.com https://www.youtube-nocookie.com; \
         img-src 'self' data: blob: https://i.ytimg.com; media-src 'self' blob:; \
         style-src 'self' 'unsafe-inline'; font-src 'self'; connect-src 'self' {} {sfu} {sfu_https}; \
         object-src 'none'; base-uri 'self'; frame-ancestors 'none'",
        sockets.join(" ")
    )
}

/// Hashed build assets never change; HTML must always be revalidated so a
/// deploy never leaves browsers pointing at deleted bundles.
async fn cache_headers(req: axum::extract::Request, next: axum::middleware::Next) -> axum::response::Response {
    let immutable = req.uri().path().starts_with("/assets/");
    let mut res = next.run(req).await;
    let is_html = res
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/html"));
    let value = if immutable && res.status().is_success() {
        Some("public, max-age=31536000, immutable")
    } else if is_html {
        Some("no-cache")
    } else {
        None
    };
    if let Some(v) = value {
        res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(v));
    }
    res
}

pub fn router(app: AppState) -> Router {
    let x_request_id = HeaderName::from_static("x-request-id");

    let web_api = Router::new()
        .route("/rooms", post(rooms::create_room))
        .route("/rooms/{slug}", get(rooms::get_room))
        .route("/rooms/{slug}/join", post(rooms::join_room))
        .route("/rooms/{slug}/settings", patch(rooms::update_settings))
        .route("/rooms/{slug}/search", get(rooms::search))
        .route("/rooms/{slug}/push", post(rooms::subscribe_push).delete(rooms::unsubscribe_push))
        .route("/push/key", get(rooms::push_key))
        .route("/rooms/{slug}/ws", get(ws_handler))
        .layer(cors(&app));

    // Developer API: server-to-server, so no CORS (keys must not live in browsers).
    let developer_api = Router::new()
        .route("/keys", post(developer::create_key))
        .route("/rooms", post(developer::create_room))
        .route("/rooms/{slug}", get(developer::room_status))
        .route("/rooms/{slug}/play", post(developer::play))
        .route("/rooms/{slug}/queue", post(developer::add_to_queue));

    let mut router = Router::new()
        .route("/healthz", get(health::health))
        .route("/readyz", get(health::ready))
        .route(
            "/api/openapi.yaml",
            get(|| async { ([(header::CONTENT_TYPE, "application/yaml")], OPENAPI) }),
        )
        .nest("/api", web_api)
        .nest("/api/v1", developer_api)
        .layer(DefaultBodyLimit::max(BODY_LIMIT));

    if let Some(dir) = &app.config.static_dir {
        let index = dir.join("index.html");
        router = router
            .fallback_service(ServeDir::new(dir).fallback(ServeFile::new(index)))
            .layer(axum::middleware::from_fn(cache_headers));
    }

    router
        .layer(SetResponseHeaderLayer::if_not_present(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_str(&csp(&app)).expect("CSP is ASCII"),
        ))
        .layer(SetResponseHeaderLayer::overriding(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")))
        .layer(SetResponseHeaderLayer::overriding(header::REFERRER_POLICY, HeaderValue::from_static("strict-origin-when-cross-origin")))
        .layer(SetResponseHeaderLayer::overriding(
            HeaderName::from_static("permissions-policy"),
            HeaderValue::from_static("camera=(self), microphone=(self), autoplay=(self \"https://www.youtube.com\"), fullscreen=(self \"https://www.youtube.com\")"),
        ))
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::new(x_request_id.clone()))
        .layer(SetRequestIdLayer::new(x_request_id, MakeRequestUuid))
        .with_state(app)
}

fn cors(app: &AppState) -> CorsLayer {
    let origins: Vec<HeaderValue> = app
        .config
        .allowed_origins
        .iter()
        .filter_map(|o| HeaderValue::from_str(o).ok())
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::POST, Method::PATCH, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
}
