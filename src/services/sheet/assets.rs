use axum::{
    Router,
    body::Body,
    http::{Response, header::HeaderValue},
    routing::get,
};

pub(super) fn routes() -> Router {
    Router::new()
        .route(
            "/",
            get(|| async { asset(include_str!("web/index.html"), "text/html; charset=utf-8") }),
        )
        .route(
            "/style.css",
            get(|| async { asset(include_str!("web/style.css"), "text/css") }),
        )
        .route(
            "/app.js",
            get(|| async { asset(include_str!("web/app.js"), "text/javascript") }),
        )
        .route(
            "/view.js",
            get(|| async { asset(include_str!("web/view.js"), "text/javascript") }),
        )
        .route(
            "/drawing.js",
            get(|| async { asset(include_str!("web/drawing.js"), "text/javascript") }),
        )
        .route(
            "/inspector.js",
            get(|| async { asset(include_str!("web/inspector.js"), "text/javascript") }),
        )
        .route(
            "/trajectory.js",
            get(|| async { asset(include_str!("web/trajectory.js"), "text/javascript") }),
        )
        .route(
            "/study.js",
            get(|| async { asset(include_str!("web/study.js"), "text/javascript") }),
        )
        .route(
            "/controls.js",
            get(|| async { asset(include_str!("web/controls.js"), "text/javascript") }),
        )
}

fn asset(content: &'static str, content_type: &'static str) -> Response<Body> {
    let mut response = Response::new(Body::from(content));
    let headers = response.headers_mut();
    headers.insert("content-type", HeaderValue::from_static(content_type));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    headers.insert("content-security-policy",HeaderValue::from_static(
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"));
    response
}
