use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::json;
use stunbeacon::{build_app, AppState};
use tempfile::tempdir;
use tower::ServiceExt;

fn test_state(token: &str) -> AppState {
    AppState::new(token)
}

fn persistent_test_state(
    token: &str,
    data_file: impl AsRef<std::path::Path>,
) -> std::io::Result<AppState> {
    AppState::new_persistent(token, data_file)
}

#[tokio::test]
async fn get_returns_404_when_channel_addr_is_empty() {
    let app = build_app(test_state("secret-token"));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/stun/demo/get")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_rejects_request_without_valid_bearer_token() {
    let app = build_app(test_state("secret-token"));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/stun/demo/update")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"addr":"1.2.3.4:5678"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn update_rejects_request_with_wrong_bearer_token() {
    let app = build_app(test_state("secret-token"));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/stun/demo/update")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer wrong-token")
                .body(Body::from(r#"{"addr":"1.2.3.4:5678"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn update_stores_addr_and_get_returns_plain_text_for_same_channel() {
    let app = build_app(test_state("secret-token"));

    let update_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/stun/demo/update")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::from(r#"{"addr":"1.2.3.4:5678"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(update_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .oneshot(
            Request::builder()
                .uri("/api/stun/demo/get")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(get_response.status(), StatusCode::OK);

    let body = get_response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), b"1.2.3.4:5678");
}

#[tokio::test]
async fn update_keeps_channels_isolated() {
    let app = build_app(test_state("secret-token"));

    let alpha_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/stun/alpha/update")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::from(r#"{"addr":"1.1.1.1:1111"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(alpha_response.status(), StatusCode::NO_CONTENT);

    let beta_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/stun/beta/update")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::from(r#"{"addr":"2.2.2.2:2222"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(beta_response.status(), StatusCode::NO_CONTENT);

    let alpha_get = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/stun/alpha/get")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(alpha_get.status(), StatusCode::OK);
    let alpha_body = alpha_get.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(alpha_body.as_ref(), b"1.1.1.1:1111");

    let beta_get = app
        .oneshot(
            Request::builder()
                .uri("/api/stun/beta/get")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(beta_get.status(), StatusCode::OK);
    let beta_body = beta_get.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(beta_body.as_ref(), b"2.2.2.2:2222");
}

#[tokio::test]
async fn update_overwrites_addr_within_same_channel() {
    let app = build_app(test_state("secret-token"));

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/stun/demo/update")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::from(r#"{"addr":"1.2.3.4:5678"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first_response.status(), StatusCode::NO_CONTENT);

    let second_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/stun/demo/update")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::from(r#"{"addr":"5.6.7.8:9999"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .oneshot(
            Request::builder()
                .uri("/api/stun/demo/get")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);
    let body = get_response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), b"5.6.7.8:9999");
}

#[tokio::test]
async fn gost_nodes_endpoint_defaults_to_socks5_tls_node_json() {
    let app = build_app(test_state("secret-token"));

    let update_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/stun/demo/update")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::from(r#"{"addr":"5.6.7.8:9999"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::NO_CONTENT);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/stun/demo/gost/nodes?username=demo-user&password=pwd&serverName=home.example.com&caFile=%2Fpath%2Fto%2Fca.pem")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        payload,
        json!([
            {
                "name": "demo",
                "addr": "5.6.7.8:9999",
                "connector": {
                    "type": "socks5",
                    "auth": {
                        "username": "demo-user",
                        "password": "pwd"
                    }
                },
                "dialer": {
                    "type": "tls",
                    "tls": {
                        "caFile": "/path/to/ca.pem",
                        "secure": true,
                        "serverName": "home.example.com"
                    }
                }
            }
        ])
    );
}

#[tokio::test]
async fn gost_nodes_endpoint_keeps_explicit_ss_tcp_compatibility() {
    let app = build_app(test_state("secret-token"));

    let update_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/stun/demo/update")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::from(r#"{"addr":"5.6.7.8:9999"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::NO_CONTENT);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/stun/demo/gost/nodes?connector=ss&dialer=tcp&username=chacha20-ietf-poly1305&password=pwd")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        payload,
        json!([
            {
                "name": "demo",
                "addr": "5.6.7.8:9999",
                "connector": {
                    "type": "ss",
                    "auth": {
                        "username": "chacha20-ietf-poly1305",
                        "password": "pwd"
                    }
                },
                "dialer": {
                    "type": "tcp"
                }
            }
        ])
    );
}

#[tokio::test]
async fn gost_nodes_endpoint_rejects_incomplete_auth_query() {
    let app = build_app(test_state("secret-token"));

    let update_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/stun/demo/update")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::from(r#"{"addr":"5.6.7.8:9999"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::NO_CONTENT);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/stun/demo/gost/nodes?username=chacha20-ietf-poly1305")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn update_persists_channels_to_data_file() {
    let temp_dir = tempdir().unwrap();
    let data_file = temp_dir.path().join("channels.json");
    let app = build_app(persistent_test_state("secret-token", &data_file).unwrap());

    let update_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/stun/demo/update")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::from(r#"{"addr":"9.8.7.6:4321"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(update_response.status(), StatusCode::NO_CONTENT);

    let persisted: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&data_file).unwrap()).unwrap();
    assert_eq!(
        persisted,
        json!({
            "version": 1,
            "channels": {
                "demo": "9.8.7.6:4321"
            }
        })
    );
}

#[tokio::test]
async fn persistent_state_restores_channel_after_restart() {
    let temp_dir = tempdir().unwrap();
    let data_file = temp_dir.path().join("channels.json");
    let first_app = build_app(persistent_test_state("secret-token", &data_file).unwrap());

    let update_response = first_app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/stun/demo/update")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::from(r#"{"addr":"9.8.7.6:4321"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update_response.status(), StatusCode::NO_CONTENT);

    let restarted_app = build_app(persistent_test_state("secret-token", &data_file).unwrap());

    let get_response = restarted_app
        .oneshot(
            Request::builder()
                .uri("/api/stun/demo/get")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(get_response.status(), StatusCode::OK);
    let body = get_response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.as_ref(), b"9.8.7.6:4321");
}

#[tokio::test]
async fn update_returns_500_and_keeps_memory_clean_when_persist_fails() {
    let temp_dir = tempdir().unwrap();
    let readonly_dir = temp_dir.path().join("readonly");
    std::fs::create_dir(&readonly_dir).unwrap();
    let mut permissions = std::fs::metadata(&readonly_dir).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&readonly_dir, permissions).unwrap();
    let data_file = readonly_dir.join("channels.json");
    let app = build_app(persistent_test_state("secret-token", &data_file).unwrap());

    let update_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/stun/demo/update")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::AUTHORIZATION, "Bearer secret-token")
                .body(Body::from(r#"{"addr":"9.8.7.6:4321"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(update_response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let get_response = app
        .oneshot(
            Request::builder()
                .uri("/api/stun/demo/get")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let mut permissions = std::fs::metadata(&readonly_dir).unwrap().permissions();
    permissions.set_readonly(false);
    std::fs::set_permissions(&readonly_dir, permissions).unwrap();

    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
}
