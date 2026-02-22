use axum::http::{header, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "dashboard/dist"]
#[cfg_attr(not(feature = "dashboard"), exclude = "**/*")]
struct DashboardAssets;

pub fn static_router() -> Router {
    Router::new().fallback(get(serve_static))
}

async fn serve_static(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    // Try exact path first, then fall back to index.html for SPA routing
    let file = DashboardAssets::get(path).or_else(|| {
        if path.is_empty() || !path.contains('.') {
            DashboardAssets::get("index.html")
        } else {
            None
        }
    });

    match file {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let mime_str = if path.is_empty() || !path.contains('.') {
                "text/html; charset=utf-8".to_string()
            } else {
                mime.to_string()
            };

            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, mime_str),
                    (
                        header::CACHE_CONTROL,
                        if path.contains("assets/") {
                            "public, max-age=31536000, immutable".to_string()
                        } else {
                            "no-cache".to_string()
                        },
                    ),
                ],
                content.data.to_vec(),
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
