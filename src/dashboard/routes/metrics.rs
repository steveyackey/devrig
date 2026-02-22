use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::Json;

use crate::otel::query::{MetricQuery, MetricSeriesQuery};

use super::DashboardState;

pub async fn list_metrics(
    State(state): State<DashboardState>,
    Query(query): Query<MetricQuery>,
) -> impl IntoResponse {
    let store = state.store.read().await;
    let metrics = store.query_metrics(&query);
    Json(metrics).into_response()
}

pub async fn get_metric_series(
    State(state): State<DashboardState>,
    Query(query): Query<MetricSeriesQuery>,
) -> impl IntoResponse {
    let store = state.store.read().await;
    let response = store.query_metric_series(&query);
    Json(response).into_response()
}
