//! REST endpoints used by the Partello web app.

use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode, header},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AppState,
    error::{ApiJson, ApiQuery, AppError, AppResult},
    rate_limit::client_ip_with,
    realtime::hub::SettingsChange,
    room::model::{RoomInfo, RoomSettings},
    validation,
};

#[derive(Deserialize)]
pub struct CreateRoomRequest {
    /// Optional; defaults to "<display_name>'s room".
    pub name: Option<String>,
    pub display_name: String,
    #[serde(default)]
    pub settings: Option<RoomSettings>,
    /// Optional first video (e.g. picked from the landing page's genre grid).
    pub video_url: Option<String>,
    /// Optional 4-digit room PIN.
    pub pin: Option<String>,
}

#[derive(Serialize)]
pub struct CreateRoomResponse {
    pub room: RoomInfo,
    /// Secret that makes its holder the host. Shown once.
    pub host_key: String,
    pub invite_url: String,
}

#[derive(Serialize)]
pub struct RoomSummaryResponse {
    pub room: RoomInfo,
    pub participant_count: usize,
    pub is_full: bool,
}

#[derive(Deserialize)]
pub struct JoinRequest {
    pub display_name: String,
    pub host_key: Option<String>,
    pub pin: Option<String>,
}

#[derive(Serialize)]
pub struct JoinResponse {
    pub participant_id: Uuid,
    /// Bearer token for the WebSocket `auth` frame and host-only REST calls.
    pub session_token: String,
    pub is_creator: bool,
    pub room: RoomInfo,
}

pub(crate) fn ip(app: &AppState, headers: &HeaderMap, peer: SocketAddr) -> std::net::IpAddr {
    client_ip_with(headers, Some(peer), app.config.trust_proxy, app.config.client_ip_header.as_deref())
}

pub(crate) fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim)
        .filter(|t| !t.is_empty() && t.len() <= 200)
}

pub async fn create_room(
    State(app): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ApiJson(req): ApiJson<CreateRoomRequest>,
) -> AppResult<(StatusCode, Json<CreateRoomResponse>)> {
    if !app.limiters.create_room.check(&ip(&app, &headers, peer)) {
        return Err(AppError::RateLimited);
    }
    let display_name = validation::display_name(&req.display_name)?;
    let name = match req.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => n.to_string(),
        None => default_room_name(&display_name),
    };
    // Resolve (and so validate) the video before creating anything, so a bad
    // link never leaves behind a room nobody holds the host key for.
    let item = match &req.video_url {
        Some(url) => Some(app.hub.resolve_video(url, Uuid::nil(), "Partello".into()).await?),
        None => None,
    };
    let pin = req.pin.as_deref().filter(|p| !p.is_empty());
    let (record, host_key) = app.hub.create_room(&name, req.settings.unwrap_or_default(), None, pin).await?;
    if let Some(item) = item {
        app.hub.seed_item(&record.slug, item).await?;
    }
    let invite_url = format!("{}/r/{}", app.config.public_base_url, record.slug);
    Ok((
        StatusCode::CREATED,
        Json(CreateRoomResponse {
            room: RoomInfo::from(&record),
            host_key,
            invite_url,
        }),
    ))
}

fn default_room_name(display_name: &str) -> String {
    let base: String = display_name.chars().take(validation::ROOM_NAME_MAX - 7).collect();
    format!("{base}'s room")
}

pub async fn get_room(State(app): State<AppState>, Path(slug): Path<String>) -> AppResult<Json<RoomSummaryResponse>> {
    let (room, participant_count, is_full) = app.hub.room_summary(&slug).await?;
    Ok(Json(RoomSummaryResponse {
        is_full,
        room,
        participant_count,
    }))
}

pub async fn join_room(
    State(app): State<AppState>,
    Path(slug): Path<String>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ApiJson(req): ApiJson<JoinRequest>,
) -> AppResult<Json<JoinResponse>> {
    let client = ip(&app, &headers, peer);
    if !app.limiters.join.check(&client) {
        return Err(AppError::RateLimited);
    }
    // PIN guesses are limited (and rooms lock after repeated misses) inside
    // the hub, where only rooms that really have a PIN are counted.
    let pin = req.pin.as_deref().filter(|p| !p.is_empty() && p.len() <= 16);
    let host_key = req.host_key.as_deref().filter(|k| k.len() <= 200);
    let out = app.hub.join(&slug, &req.display_name, host_key, pin, client).await?;
    Ok(Json(JoinResponse {
        participant_id: out.participant_id,
        session_token: out.token,
        is_creator: out.is_host_key_valid,
        room: out.room,
    }))
}

pub async fn update_settings(
    State(app): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    ApiJson(change): ApiJson<SettingsChange>,
) -> AppResult<Json<RoomInfo>> {
    let token = bearer(&headers).ok_or(AppError::Unauthorized)?;
    let room = app.hub.update_settings(&slug, token, change).await?;
    Ok(Json(room))
}

#[derive(Deserialize)]
pub struct SearchParams {
    pub q: String,
}

#[derive(Serialize)]
pub struct SearchResponse {
    pub results: Vec<crate::youtube_search::SearchResult>,
}

/// YouTube search for room members (session token required, so the server's
/// YouTube quota is only spent on people actually in a room).
pub async fn search(
    State(app): State<AppState>,
    Path(slug): Path<String>,
    ApiQuery(params): ApiQuery<SearchParams>,
    headers: HeaderMap,
) -> AppResult<Json<SearchResponse>> {
    let token = bearer(&headers).ok_or(AppError::Unauthorized)?;
    let session = app.hub.authenticate(&slug, token)?;
    if !app.limiters.search.check(&session.participant_id) {
        return Err(AppError::RateLimited);
    }
    let results = app.search.search(&params.q).await?;
    Ok(Json(SearchResponse { results }))
}
