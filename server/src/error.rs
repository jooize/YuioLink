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
        match self {
            // The friendly, by-design 404: links are ephemeral.
            AppError::NotFound => (
                StatusCode::NOT_FOUND,
                Html(views::not_found_page().into_string()),
            )
                .into_response(),
            AppError::Internal(detail) => {
                tracing::error!(error = %detail, "internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Html(views::error_page(500, "Something went wrong.").into_string()),
                )
                    .into_response()
            }
        }
    }
}
