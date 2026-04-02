use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    channel_addrs: Arc<RwLock<HashMap<String, String>>>,
    auth_token: Arc<String>,
}

impl AppState {
    pub fn new(auth_token: impl Into<String>) -> Self {
        Self {
            channel_addrs: Arc::new(RwLock::new(HashMap::new())),
            auth_token: Arc::new(auth_token.into()),
        }
    }
}

#[derive(Deserialize)]
struct UpdateRequest {
    addr: String,
}

pub fn build_app(state: AppState) -> Router {
    Router::new()
        .route("/api/stun/{channel_id}/get", get(get_addr))
        .route("/api/stun/{channel_id}/update", post(update_addr))
        .with_state(state)
}

async fn get_addr(
    Path(channel_id): Path<String>,
    State(state): State<AppState>,
) -> Result<String, StatusCode> {
    let channel_addrs = state.channel_addrs.read().await;
    match channel_addrs.get(&channel_id) {
        Some(current_addr) if !current_addr.is_empty() => Ok(current_addr.clone()),
        _ => Err(StatusCode::NOT_FOUND),
    }
}

async fn update_addr(
    Path(channel_id): Path<String>,
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<UpdateRequest>,
) -> Result<StatusCode, StatusCode> {
    if !is_authorized(&headers, state.auth_token.as_ref()) {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let next_addr = payload.addr.trim();
    if next_addr.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let mut channel_addrs = state.channel_addrs.write().await;
    channel_addrs.insert(channel_id, next_addr.to_owned());

    Ok(StatusCode::NO_CONTENT)
}

fn is_authorized(headers: &HeaderMap, expected_token: &str) -> bool {
    extract_bearer_token(headers).is_some_and(|token| token == expected_token)
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    let raw_value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    raw_value.strip_prefix("Bearer ")
}
