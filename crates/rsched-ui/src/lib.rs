//! rsched-ui — static assets baked into the binary (rust-embed) + axum
//! handler that serves them under `/`.

#![warn(missing_docs)]

use axum::body::Body;
use axum::extract::Path;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rust_embed::RustEmbed;

/// Embedded UI assets. The folder ships inside the binary at build time so
/// the server has zero runtime dependency on a static-file directory.
#[derive(RustEmbed)]
#[folder = "assets/"]
struct Assets;

/// Build the static-file router mounted at `/`.
pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/*path", get(asset))
}

async fn index() -> Response {
    serve("index.html").into_response()
}

async fn asset(Path(path): Path<String>) -> Response {
    serve(&path).into_response()
}

fn serve(path: &str) -> Response {
    match Assets::get(path) {
        Some(f) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let mut resp = Response::new(Body::from(f.data));
            resp.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_str(mime.as_ref()).unwrap(),
            );
            resp
        }
        None => (StatusCode::NOT_FOUND, format!("not found: {path}")).into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn serves_index() {
        let app = router();
        let r = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let bytes = axum::body::to_bytes(r.into_body(), 65536).await.unwrap();
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("rusty-sched"));
    }

    #[tokio::test]
    async fn unknown_path_404() {
        let app = router();
        let r = app
            .oneshot(
                Request::builder()
                    .uri("/nope.css")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), 404);
    }
}
