#![forbid(unsafe_code)]

use axum::{
    extract::{
        rejection::{JsonRejection, PathRejection, QueryRejection},
        FromRequest, FromRequestParts, Path, Query, Request,
    },
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub error_code: String,
    pub message: String,
    pub details: Value,
    pub request_id: Uuid,
}

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub error_code: &'static str,
    pub message: String,
    pub details: Value,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({}): {}", self.error_code, self.status, self.message)
    }
}

impl std::error::Error for ApiError {}

impl ApiError {
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            error_code: "UNAUTHORIZED",
            message: message.into(),
            details: json!({}),
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            error_code: "FORBIDDEN",
            message: message.into(),
            details: json!({}),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            error_code: "NOT_FOUND",
            message: message.into(),
            details: json!({}),
        }
    }

    pub fn conflict(error_code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            error_code,
            message: message.into(),
            details: json!({}),
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            error_code: "VALIDATION_ERROR",
            message: message.into(),
            details: json!({}),
        }
    }

    pub fn validation_with_details(message: impl Into<String>, details: Value) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            error_code: "VALIDATION_ERROR",
            message: message.into(),
            details,
        }
    }

    pub fn rate_limited(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            error_code: "RATE_LIMITED",
            message: message.into(),
            details: json!({}),
        }
    }

    pub fn internal(err: impl std::fmt::Display) -> Self {
        tracing::error!(error = %err, "internal API error");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            error_code: "INTERNAL_ERROR",
            message: "Internal server error".to_string(),
            details: json!({}),
        }
    }

    pub fn from_db(err: sqlx::Error) -> Self {
        if let Some(db_err) = err.as_database_error() {
            match db_err.code().as_deref() {
                // unique_violation -> 409; check/fk/not_null -> 422
                Some("23505") => {
                    return Self::conflict("CONFLICT", "Unique constraint violation");
                }
                Some("23503") | Some("23514") | Some("23502") => {
                    return Self::validation("Database integrity constraint failed");
                }
                _ => {}
            }
        }
        Self::internal(err)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = ApiErrorBody {
            error_code: self.error_code.to_string(),
            message: self.message,
            details: self.details,
            request_id: Uuid::new_v4(),
        };
        (self.status, Json(body)).into_response()
    }
}

/// JSON extractor converting serde/deserialization errors into canonical HTTP 422 responses.
pub struct ValidatedJson<T>(pub T);

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned,
    Json<T>: FromRequest<S, Rejection = JsonRejection>,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(val)) => Ok(Self(val)),
            Err(rej) => Err(ApiError::validation(rej.body_text())),
        }
    }
}

/// Query extractor converting malformed query parameters into canonical HTTP 422 responses.
pub struct ValidatedQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ValidatedQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
    Query<T>: FromRequestParts<S, Rejection = QueryRejection>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Query::<T>::from_request_parts(parts, state).await {
            Ok(Query(val)) => Ok(Self(val)),
            Err(rej) => Err(ApiError::validation(rej.body_text())),
        }
    }
}

/// Path extractor converting malformed UUID/path segments into canonical HTTP 422 responses.
pub struct ValidatedPath<T>(pub T);

impl<S, T> FromRequestParts<S> for ValidatedPath<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Send,
    Path<T>: FromRequestParts<S, Rejection = PathRejection>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match Path::<T>::from_request_parts(parts, state).await {
            Ok(Path(val)) => Ok(Self(val)),
            Err(rej) => Err(ApiError::validation(rej.body_text())),
        }
    }
}
