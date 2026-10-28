//! Partello public developer API (`/api/v1`).
//!
//! Lets other sites and bots create Partello rooms and control what plays in
//! them. Authentication is a bearer API key; keys are self-serve, rate limited,
//! and stored only as SHA-256 hashes. A key can only act on rooms it created.

use std::net::SocketAddr;

use axum::{
    Json,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::rooms::{bearer, ip};
use crate::{
    AppState,
    database::ApiKeyRecord,
    error::{ApiJson, AppError, AppResult},
    realtime::hub::ApiRoomStatus,
    room::model::{RoomInfo, RoomSettings},
    security, validation,
};

pub const MAX_ITEMS_PER_CALL: usize = 50;

#[derive(Deserialize)]
pub struct CreateKeyRequest {
    pub label: String,
}

#[derive(Serialize)]
pub struct CreateKeyResponse {
    pub id: Uuid,
    pub label: String,
    /// Shown once. Store it server-side; never ship it in browser code.
    pub api_key: String,
}

pub async fn create_key(
    State(app): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ApiJson(req): ApiJson<CreateKeyRequest>,
) -> AppResult<(StatusCode, Json<CreateKeyResponse>)> {
    if !app.limiters.create_key.check(&ip(&app, &headers, peer)) {
        return Err(AppError::RateLimited);
    }
    let label = validation::clean_line(&req.label, "Label", 60)?;
    let api_key = security::new_secret("ptk_");
    let record = ApiKeyRecord {
        id: Uuid::new_v4(),
        label: label.clone(),
        revoked: false,
    };
    app.hub
        .store
        .insert_api_key(&record, &security::hash_secret(&api_key), &api_key[..8])
        .await?;
    tracing::info!(key_id = %record.id, "api key created");
    Ok((
        StatusCode::CREATED,
        Json(CreateKeyResponse {
            id: record.id,
            label,
            api_key,
        }),
    ))
}

async fn authorize(app: &AppState, headers: &HeaderMap) -> AppResult<ApiKeyRecord> {
    let key = bearer(headers).filter(|k| k.starts_with("ptk_")).ok_or(AppError::Unauthorized)?;
    let record = app
        .hub
        .store
        .api_key_by_hash(&security::hash_secret(key))
        .await?
        .filter(|r| !r.revoked)
        .ok_or(AppError::Unauthorized)?;
    if !app.limiters.api_calls.check(&record.id) {
        return Err(AppError::RateLimited);
    }
    Ok(record)
}

#[derive(Deserialize, Default)]
pub struct ApiCreateRoomRequest {
    pub name: Option<String>,
    /// Optional first video, starts playing when people arrive.
    pub video_url: Option<String>,
    pub settings: Option<RoomSettings>,
    /// Optional 4-digit PIN that viewers must enter (the host URL bypasses it).
    pub pin: Option<String>,
}

#[derive(Serialize)]
pub struct ApiCreateRoomResponse {
    pub room: RoomInfo,
    pub invite_url: String,
    /// Give this to the person who should host (append `#host=<key>` to the invite URL).
    pub host_key: String,
    pub host_url: String,
}

pub async fn create_room(
    State(app): State<AppState>,
    headers: HeaderMap,
    body: Option<ApiJson<ApiCreateRoomRequest>>,
) -> AppResult<(StatusCode, Json<ApiCreateRoomResponse>)> {
    let key = authorize(&app, &headers).await?;
    let req = body.map(|b| b.0).unwrap_or_default();
    let name = req.name.unwrap_or_else(|| "Watch party".into());
    let pin = req.pin.as_deref().filter(|p| !p.is_empty());
    let (record, host_key) = app
        .hub
        .create_room(&name, req.settings.unwrap_or_default(), Some(key.id), pin)
        .await?;
    if let Some(url) = req.video_url {
        if let Err(err) = app.hub.api_enqueue(&record.slug, key.id, &[url], true).await {
            // The room exists either way; report the video problem clearly.
            tracing::debug!(error = %err, "initial video rejected");
            return Err(err);
        }
    }
    let invite_url = format!("{}/r/{}", app.config.public_base_url, record.slug);
    Ok((
        StatusCode::CREATED,
        Json(ApiCreateRoomResponse {
            room: RoomInfo::from(&record),
            host_url: format!("{invite_url}#host={host_key}"),
            invite_url,
            host_key,
        }),
    ))
}

pub async fn room_status(State(app): State<AppState>, Path(slug): Path<String>, headers: HeaderMap) -> AppResult<Json<ApiRoomStatus>> {
    let key = authorize(&app, &headers).await?;
    Ok(Json(app.hub.api_status(&slug, key.id).await?))
}

#[derive(Deserialize)]
pub struct PlayRequest {
    pub url: String,
}

/// Replaces what is playing right now.
pub async fn play(
    State(app): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    ApiJson(req): ApiJson<PlayRequest>,
) -> AppResult<Json<ApiRoomStatus>> {
    let key = authorize(&app, &headers).await?;
    Ok(Json(app.hub.api_enqueue(&slug, key.id, &[req.url], true).await?))
}

#[derive(Deserialize)]
pub struct QueueItemRequest {
    pub url: String,
}

#[derive(Deserialize)]
pub struct QueueRequest {
    pub items: Vec<QueueItemRequest>,
}

/// Appends up to 50 videos to the queue.
pub async fn add_to_queue(
    State(app): State<AppState>,
    Path(slug): Path<String>,
    headers: HeaderMap,
    ApiJson(req): ApiJson<QueueRequest>,
) -> AppResult<Json<ApiRoomStatus>> {
    let key = authorize(&app, &headers).await?;
    if req.items.is_empty() || req.items.len() > MAX_ITEMS_PER_CALL {
        return Err(AppError::validation(format!("Send between 1 and {MAX_ITEMS_PER_CALL} items")));
    }
    let urls: Vec<String> = req.items.into_iter().map(|i| i.url).collect();
    Ok(Json(app.hub.api_enqueue(&slug, key.id, &urls, false).await?))
}
