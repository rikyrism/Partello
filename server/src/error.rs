//! Error type shared by HTTP handlers and the realtime layer.
//!
//! Every error has a stable machine-readable `code` (used by the frontend and
//! documented in docs/API.md) and a human-readable message that is safe to show.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Validation(String),
    /// A request the extractors could not read (bad JSON, wrong content type);
    /// keeps the precise HTTP status (400, 413, 415, 422).
    #[error("{1}")]
    Rejected(StatusCode, String),
    #[error("room not found")]
    RoomNotFound,
    #[error("this room is full")]
    RoomFull,
    #[error("your session is invalid or expired")]
    Unauthorized,
    #[error("{0}")]
    Forbidden(String),
    #[error("too many requests, slow down")]
    RateLimited,
    #[error("{0}")]
    InvalidVideo(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Unavailable(String),
    #[error("this room is already open in too many tabs")]
    TooManyConnections,
    #[error("This room needs a PIN")]
    PinRequired,
    #[error("That PIN is not right")]
    WrongPin,
    #[error("This room's PIN is locked after too many wrong tries. Ask the host to set a new one.")]
    PinLocked,
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Validation(_) | AppError::Rejected(..) => "validation_failed",
            AppError::RoomNotFound => "room_not_found",
            AppError::RoomFull => "room_full",
            AppError::Unauthorized => "unauthorized",
            AppError::Forbidden(_) => "forbidden",
            AppError::RateLimited => "rate_limited",
            AppError::InvalidVideo(_) => "invalid_video",
            AppError::Conflict(_) => "conflict",
            AppError::Unavailable(_) => "unavailable",
            AppError::TooManyConnections => "too_many_connections",
            AppError::PinRequired => "pin_required",
            AppError::WrongPin => "wrong_pin",
            AppError::PinLocked => "pin_locked",
            AppError::Internal(_) => "internal",
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            AppError::Validation(_) | AppError::InvalidVideo(_) => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::Rejected(status, _) => *status,
            AppError::RoomNotFound => StatusCode::NOT_FOUND,
            AppError::RoomFull | AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            AppError::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            AppError::TooManyConnections => StatusCode::CONFLICT,
            AppError::PinRequired => StatusCode::UNAUTHORIZED,
            AppError::WrongPin | AppError::PinLocked => StatusCode::FORBIDDEN,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn forbidden(msg: impl Into<String>) -> Self {
        AppError::Forbidden(msg.into())
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        AppError::Validation(msg.into())
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Internal(err.into())
    }
}

#[derive(Serialize)]
pub struct ErrorBody<'a> {
    pub error: ErrorDetail<'a>,
}

#[derive(Serialize)]
pub struct ErrorDetail<'a> {
    pub code: &'a str,
    pub message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let AppError::Internal(err) = &self {
            tracing::error!(error = ?err, "internal error");
        }
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.code(),
                message: self.to_string(),
            },
        };
        (self.status(), Json(body)).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;

/// JSON body extractor whose rejections use the standard error body
/// (`{"error":{"code":"validation_failed",...}}`) instead of Axum's plain text.
pub struct ApiJson<T>(pub T);

impl<S, T> axum::extract::FromRequest<S> for ApiJson<T>
where
    axum::Json<T>: axum::extract::FromRequest<S, Rejection = axum::extract::rejection::JsonRejection>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::Json::<T>::from_request(req, state).await {
            Ok(axum::Json(value)) => Ok(ApiJson(value)),
            Err(rejection) => Err(AppError::Rejected(rejection.status(), rejection.body_text())),
        }
    }
}

impl<S, T> axum::extract::OptionalFromRequest<S> for ApiJson<T>
where
    axum::Json<T>: axum::extract::OptionalFromRequest<S, Rejection = axum::extract::rejection::JsonRejection>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Option<Self>, Self::Rejection> {
        match <axum::Json<T> as axum::extract::OptionalFromRequest<S>>::from_request(req, state).await {
            Ok(value) => Ok(value.map(|axum::Json(v)| ApiJson(v))),
            Err(rejection) => Err(AppError::Rejected(rejection.status(), rejection.body_text())),
        }
    }
}

/// Query-string extractor with the standard error body.
pub struct ApiQuery<T>(pub T);

impl<S, T> axum::extract::FromRequestParts<S> for ApiQuery<T>
where
    axum::extract::Query<T>: axum::extract::FromRequestParts<S, Rejection = axum::extract::rejection::QueryRejection>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut axum::http::request::Parts, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Query::<T>::from_request_parts(parts, state).await {
            Ok(axum::extract::Query(value)) => Ok(ApiQuery(value)),
            Err(rejection) => Err(AppError::Rejected(rejection.status(), rejection.body_text())),
        }
    }
}
