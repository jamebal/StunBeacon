use std::{collections::HashMap, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
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

#[derive(Deserialize)]
struct GostNodeQuery {
    #[serde(default = "default_connector_type")]
    connector: String,
    #[serde(default = "default_dialer_type")]
    dialer: String,
    #[serde(default = "default_tls_secure")]
    secure: bool,
    username: Option<String>,
    password: Option<String>,
    name: Option<String>,
    #[serde(rename = "serverName")]
    server_name: Option<String>,
    #[serde(rename = "caFile")]
    ca_file: Option<String>,
}

#[derive(Serialize)]
struct GostNode {
    name: String,
    addr: String,
    connector: GostConnector,
    dialer: GostDialer,
}

#[derive(Serialize)]
struct GostConnector {
    #[serde(rename = "type")]
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth: Option<GostAuth>,
}

#[derive(Serialize)]
struct GostDialer {
    #[serde(rename = "type")]
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tls: Option<GostTlsConfig>,
}

#[derive(Serialize)]
struct GostAuth {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct GostTlsConfig {
    #[serde(rename = "caFile", skip_serializing_if = "Option::is_none")]
    ca_file: Option<String>,
    secure: bool,
    #[serde(rename = "serverName", skip_serializing_if = "Option::is_none")]
    server_name: Option<String>,
}

pub fn build_app(state: AppState) -> Router {
    Router::new()
        .route("/api/stun/{channel_id}/get", get(get_addr))
        .route("/api/stun/{channel_id}/gost/nodes", get(get_gost_nodes))
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

async fn get_gost_nodes(
    Path(channel_id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<GostNodeQuery>,
) -> Result<Json<Vec<GostNode>>, StatusCode> {
    let channel_addrs = state.channel_addrs.read().await;
    let current_addr = match channel_addrs.get(&channel_id) {
        Some(current_addr) if !current_addr.is_empty() => current_addr.clone(),
        _ => return Err(StatusCode::NOT_FOUND),
    };

    let connector_type = query.connector.trim();
    let dialer_type = query.dialer.trim();
    if connector_type.is_empty() || dialer_type.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let auth = match (
        normalize_optional_string(query.username),
        normalize_optional_string(query.password),
    ) {
        (Some(username), Some(password)) => Some(GostAuth { username, password }),
        (None, None) => None,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    let node_name = normalize_optional_string(query.name).unwrap_or_else(|| channel_id.clone());
    let tls = if dialer_type.eq_ignore_ascii_case("tls") {
        Some(GostTlsConfig {
            ca_file: normalize_optional_string(query.ca_file),
            secure: query.secure,
            server_name: normalize_optional_string(query.server_name),
        })
    } else {
        None
    };

    Ok(Json(vec![GostNode {
        name: node_name,
        addr: current_addr,
        connector: GostConnector {
            kind: connector_type.to_owned(),
            auth,
        },
        dialer: GostDialer {
            kind: dialer_type.to_owned(),
            tls,
        },
    }]))
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

fn default_connector_type() -> String {
    "socks5".to_owned()
}

fn default_dialer_type() -> String {
    "tls".to_owned()
}

fn default_tls_secure() -> bool {
    true
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_owned())
    })
}
