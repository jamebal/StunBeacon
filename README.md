# StunBeacon

`StunBeacon` 是一个极轻量级的信令同步服务，用来在不同的 NAT 穿透客户端之间，按通道（channel）同步最新的公网 `IP:Port`。

## 功能概览

- `GET /api/stun/{channel_id}/get`
  - 返回指定通道当前保存的纯文本地址。
  - 当通道不存在或地址为空时返回 `404 Not Found`。
- `POST /api/stun/{channel_id}/update`
  - 请求体必须是 JSON，例如 `{"addr":"1.2.3.4:5678"}`。
  - 需要携带 `Authorization: Bearer <TOKEN>`。
  - `TOKEN` 从服务端环境变量 `AUTH_TOKEN` 读取。
  - 更新成功后返回 `204 No Content`，并覆盖指定通道的最新地址。

## 项目结构

```text
.
├── .github/
│   └── workflows/
│       └── release.yml
├── Cargo.toml
├── README.md
├── src/
│   ├── app.rs
│   ├── lib.rs
│   └── main.rs
└── tests/
    └── api.rs
```

## 本地运行

### 1. 设置环境变量

```bash
export AUTH_TOKEN="your-secret-token"
export LISTEN_ADDR="0.0.0.0:3000"
```

`LISTEN_ADDR` 可选，不设置时默认监听 `0.0.0.0:3000`。

### 2. 启动服务

```bash
cargo run
```

### 3. 读取某个通道的当前地址

```bash
curl http://127.0.0.1:3000/api/stun/demo/get
```

### 4. 更新某个通道的地址

```bash
curl -X POST http://127.0.0.1:3000/api/stun/demo/update \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer your-secret-token' \
  -d '{"addr":"1.2.3.4:5678"}'
```

### 5. 多客户端示例

```bash
curl -X POST http://127.0.0.1:3000/api/stun/client-a/update \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer your-secret-token' \
  -d '{"addr":"1.1.1.1:1111"}'

curl -X POST http://127.0.0.1:3000/api/stun/client-b/update \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer your-secret-token' \
  -d '{"addr":"2.2.2.2:2222"}'

curl http://127.0.0.1:3000/api/stun/client-a/get
curl http://127.0.0.1:3000/api/stun/client-b/get
```

## 触发 GitHub Release 构建

当你把带有 `v*` 前缀的 Tag 推送到 GitHub 时，`.github/workflows/release.yml` 会自动触发：

```bash
git tag v1.0.0
git push origin v1.0.0
```

工作流会自动构建以下目标：

- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-musl`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

构建完成后，压缩包会自动上传到对应的 GitHub Release 页面。
