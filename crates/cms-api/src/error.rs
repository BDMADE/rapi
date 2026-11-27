use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use cms_core::ApiError;

pub struct HttpApiError(pub ApiError);

impl From<ApiError> for HttpApiError {
    fn from(err: ApiError) -> Self {
        Self(err)
    }
}

impl IntoResponse for HttpApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.0.status_code())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let envelope = self.0.to_envelope();
        (status, Json(envelope)).into_response()
    }
}
