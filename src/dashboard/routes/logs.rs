use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::Json;

use crate::otel::query::LogQuery;

use super::DashboardState;

pub async fn list_logs(
    State(state): State<DashboardState>,
    Query(query): Query<LogQuery>,
) -> impl IntoResponse {
    let store = state.store.read().await;
    let logs = store.query_logs(&query);
    Json(logs).into_response()
}
