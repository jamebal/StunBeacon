use stunbeacon::{build_app, AppState};

#[tokio::main]
async fn main() {
    let auth_token = std::env::var("AUTH_TOKEN").expect("必须设置 AUTH_TOKEN 环境变量");
    let listen_addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_owned());
    let data_file =
        std::env::var("DATA_FILE").unwrap_or_else(|_| "./data/channels.json".to_owned());

    let listener = tokio::net::TcpListener::bind(&listen_addr)
        .await
        .expect("监听地址绑定失败");

    let state = AppState::new_persistent(auth_token, &data_file).expect("持久化数据文件加载失败");

    println!("StunBeacon 正在监听 {listen_addr}，数据文件 {data_file}");

    axum::serve(listener, build_app(state))
        .await
        .expect("HTTP 服务启动失败");
}
