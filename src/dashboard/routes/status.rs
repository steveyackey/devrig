use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;

use super::DashboardState;

pub async fn get_status(State(state): State<DashboardState>) -> impl IntoResponse {
    let store = state.store.read().await;
    let status = store.get_status();
    Json(status).into_response()
}
