use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    extract::Request,
    http::header::{HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};

static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const REQUEST_ID_HEADER: &str = "x-request-id";

tokio::task_local! {
    static ACTIVE_REQUEST_ID: String;
}

pub(crate) fn current_request_id() -> Option<String> {
    ACTIVE_REQUEST_ID
        .try_with(|request_id| request_id.clone())
        .ok()
}

pub(crate) async fn request_id_middleware(request: Request, next: Next) -> Response {
    let request_id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(generate_request_id);

    let mut response = ACTIVE_REQUEST_ID
        .scope(request_id.clone(), next.run(request))
        .await;
    response.headers_mut().insert(
        HeaderName::from_static(REQUEST_ID_HEADER),
        HeaderValue::from_str(&request_id)
            .unwrap_or_else(|_| HeaderValue::from_static("req-invalid")),
    );
    response
}

fn generate_request_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let sequence = REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed) + 1;
    format!("req-{timestamp}-{sequence:08x}")
}
