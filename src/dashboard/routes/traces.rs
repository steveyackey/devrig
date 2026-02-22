use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use crate::otel::query::TraceQuery;

use super::DashboardState;

pub async fn list_traces(
    State(state): State<DashboardState>,
    Query(query): Query<TraceQuery>,
) -> impl IntoResponse {
    let store = state.store.read().await;
    let traces = store.query_traces(&query);
    Json(traces).into_response()
}

pub async fn get_trace(
    State(state): State<DashboardState>,
    Path(trace_id): Path<String>,
) -> impl IntoResponse {
    let store = state.store.read().await;
    match store.get_trace(&trace_id) {
        Some(detail) => Json(detail).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub async fn get_related(
    State(state): State<DashboardState>,
    Path(trace_id): Path<String>,
) -> impl IntoResponse {
    let store = state.store.read().await;
    let related = store.get_related(&trace_id);
    Json(related).into_response()
}
