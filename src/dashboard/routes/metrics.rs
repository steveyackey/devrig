use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::Json;

use crate::otel::query::MetricQuery;

use super::DashboardState;

pub async fn list_metrics(
    State(state): State<DashboardState>,
    Query(query): Query<MetricQuery>,
) -> impl IntoResponse {
    let store = state.store.read().await;
    let metrics = store.query_metrics(&query);
    Json(metrics).into_response()
}
