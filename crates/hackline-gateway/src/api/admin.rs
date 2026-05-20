//! Static React admin bundle served from `/admin/*` (SCOPE.md §13
//! Phase 3). Three files (`index.html`, `admin.js`, `admin.css`) are
//! embedded with `include_str!` so the gateway ships as a single
//! static binary — there is no runtime filesystem dependency on the
//! `static/admin/` directory once the binary is built.
//!
//! The bundle is intentionally plain HTML + a single vanilla JS file:
//! the SCOPE.md Phase 3 goal is the smallest viable static admin
//! against the existing REST + SSE surface, not a new framework.

use axum::extract::Path;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

const INDEX_HTML: &str = include_str!("../../static/admin/index.html");
const ADMIN_JS: &str = include_str!("../../static/admin/admin.js");
const ADMIN_CSS: &str = include_str!("../../static/admin/admin.css");

pub async fn index() -> impl IntoResponse {
    serve_static(INDEX_HTML.as_bytes(), "text/html; charset=utf-8", false)
}

pub async fn asset(Path(name): Path<String>) -> Response {
    let (body, ct) = match name.as_str() {
        "" | "index.html" => (INDEX_HTML.as_bytes(), "text/html; charset=utf-8"),
        "admin.js" => (ADMIN_JS.as_bytes(), "application/javascript; charset=utf-8"),
        "admin.css" => (ADMIN_CSS.as_bytes(), "text/css; charset=utf-8"),
        _ => return (StatusCode::NOT_FOUND, "not found").into_response(),
    };
    // Hashed asset paths would deserve `immutable, max-age=31536000`;
    // we ship unhashed names, so a moderate cache is the right call.
    serve_static(body, ct, true).into_response()
}

fn serve_static(body: &'static [u8], ct: &'static str, cacheable: bool) -> Response {
    let cache = if cacheable {
        "public, max-age=300"
    } else {
        "no-cache"
    };
    (
        StatusCode::OK,
        [(CONTENT_TYPE, ct), (CACHE_CONTROL, cache)],
        body,
    )
        .into_response()
}
