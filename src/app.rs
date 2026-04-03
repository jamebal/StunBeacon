use std::{
    collections::HashMap,
    fs, io,
    path::{Path as FilePath, PathBuf},
    sync::{Arc, Mutex},
};

use axum::{
    extract::{Path, Query, State},
    http::{header::AUTHORIZATION, HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct AppState {
    channel_store: Arc<Mutex<ChannelStore>>,
    auth_token: Arc<String>,
}

impl AppState {
    pub fn new(auth_token: impl Into<String>) -> Self {
        Self::from_parts(auth_token, HashMap::new(), None)
    }

    pub fn new_persistent(
        auth_token: impl Into<String>,
        data_file: impl AsRef<FilePath>,
    ) -> io::Result<Self> {
        let data_file = data_file.as_ref().to_path_buf();
        let channel_addrs = load_channel_addrs(&data_file)?;
        Ok(Self::from_parts(auth_token, channel_addrs, Some(data_file)))
    }

    fn from_parts(
        auth_token: impl Into<String>,
        channel_addrs: HashMap<String, String>,
        data_file: Option<PathBuf>,
    ) -> Self {
        Self {
            channel_store: Arc::new(Mutex::new(ChannelStore {
                channel_addrs,
                data_file,
            })),
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

struct ChannelStore {
    channel_addrs: HashMap<String, String>,
    data_file: Option<PathBuf>,
}

#[derive(Deserialize, Serialize)]
struct PersistedChannels {
    version: u8,
    channels: HashMap<String, String>,
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
    let channel_store = state.channel_store.lock().unwrap();
    match channel_store.channel_addrs.get(&channel_id) {
        Some(current_addr) if !current_addr.is_empty() => Ok(current_addr.clone()),
        _ => Err(StatusCode::NOT_FOUND),
    }
}

async fn get_gost_nodes(
    Path(channel_id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<GostNodeQuery>,
) -> Result<Json<Vec<GostNode>>, StatusCode> {
    let channel_store = state.channel_store.lock().unwrap();
    let current_addr = match channel_store.channel_addrs.get(&channel_id) {
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

    let mut channel_store = state.channel_store.lock().unwrap();
    let previous_addr = channel_store
        .channel_addrs
        .insert(channel_id.clone(), next_addr.to_owned());

    if let Err(_err) = channel_store.persist() {
        restore_previous_addr(&mut channel_store.channel_addrs, channel_id, previous_addr);
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

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

impl ChannelStore {
    fn persist(&self) -> io::Result<()> {
        let Some(data_file) = &self.data_file else {
            return Ok(());
        };

        let persisted = PersistedChannels {
            version: 1,
            channels: self.channel_addrs.clone(),
        };
        write_persisted_channels(data_file, &persisted)
    }
}

fn load_channel_addrs(data_file: &FilePath) -> io::Result<HashMap<String, String>> {
    let contents = match fs::read_to_string(data_file) {
        Ok(contents) => contents,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(err) => return Err(err),
    };

    let persisted: PersistedChannels = serde_json::from_str(&contents).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("持久化文件格式错误: {err}"),
        )
    })?;

    if persisted.version != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("不支持的持久化版本: {}", persisted.version),
        ));
    }

    Ok(persisted.channels)
}

fn write_persisted_channels(data_file: &FilePath, persisted: &PersistedChannels) -> io::Result<()> {
    let parent_dir = data_file
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| FilePath::new("."));
    fs::create_dir_all(parent_dir)?;

    let mut payload = serde_json::to_vec_pretty(persisted).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("持久化数据序列化失败: {err}"),
        )
    })?;
    payload.push(b'\n');

    let temp_file = temporary_data_file_path(data_file);
    fs::write(&temp_file, payload)?;
    if let Err(err) = fs::rename(&temp_file, data_file) {
        let _ = fs::remove_file(&temp_file);
        return Err(err);
    }

    Ok(())
}

fn temporary_data_file_path(data_file: &FilePath) -> PathBuf {
    let file_name = data_file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "channels.json".to_owned());
    data_file.with_file_name(format!("{file_name}.tmp"))
}

fn restore_previous_addr(
    channel_addrs: &mut HashMap<String, String>,
    channel_id: String,
    previous_addr: Option<String>,
) {
    match previous_addr {
        Some(previous_addr) => {
            channel_addrs.insert(channel_id, previous_addr);
        }
        None => {
            channel_addrs.remove(&channel_id);
        }
    }
}
