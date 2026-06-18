//! Page-level error type. Replaces the original's `panic!`-on-every-error
//! handlers with real HTTP statuses and a styled error page.

use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};

use crate::views;

pub enum AppError {
    NotFound,
    Internal(String),
}

impl AppError {
    pub fn internal<E: std::fmt::Display>(err: E) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "That link does not exist."),
            AppError::Internal(detail) => {
                tracing::error!(error = %detail, "internal server error");
                (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong.")
            }
        };
        (
            status,
            Html(views::error_page(status.as_u16(), message).into_string()),
        )
            .into_response()
    }
}
