# StunBeacon

`StunBeacon` 是一个极轻量级的信令同步服务，用来在不同的 NAT 穿透客户端之间，按通道（channel）同步最新的公网 `IP:Port`。

## 功能概览

- `GET /api/stun/{channel_id}/get`
  - 返回指定通道当前保存的纯文本地址。
  - 当通道不存在或地址为空时返回 `404 Not Found`。
- `GET /api/stun/{channel_id}/gost/nodes`
  - 返回 `gost 3.x` 可直接消费的节点 JSON 数组。
  - 默认生成 `socks5 + tls` 节点，并默认输出 `secure=true`。
  - 可通过查询参数 `username`、`password`、`connector`、`dialer`、`name`、`serverName`、`caFile`、`secure` 调整节点内容。
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
├── .dockerignore
├── Cargo.toml
├── Dockerfile
├── Dockerfile.ghcr
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

### 6. 生成 `gost 3.x` 节点配置

先写入最新地址：

```bash
curl -X POST http://127.0.0.1:3000/api/stun/demo/update \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer your-secret-token' \
  -d '{"addr":"1.2.3.4:5678"}'
```

再读取适合家用回连场景的 `socks5 + tls` 节点数组：

```bash
curl "http://127.0.0.1:3000/api/stun/demo/gost/nodes?username=demo-user&password=pwd&serverName=home.example.com&caFile=%2Fpath%2Fto%2Fca.pem"
```

返回示例：

```json
[
  {
    "name": "demo",
    "addr": "1.2.3.4:5678",
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
]
```

说明：

- `caFile` 是客户端机器上的 CA 文件路径，服务端只是把这个字符串写入返回 JSON。
- 如果你想兼容旧用法，仍可显式传 `connector=ss&dialer=tcp` 切回旧节点格式。

### 7. 与 `gost 3.26` 集成

`gost 3.26` 要想自动切换最新地址，推荐使用配置文件里的 `hop.http` 数据源和 `reload`，不要把上游地址写死在命令行里。

```yaml
services:
  - name: socks5
    addr: ":1080"
    handler:
      type: socks5
      chain: chain-0
    listener:
      type: tcp

chains:
  - name: chain-0
    hops:
      - name: beacon-hop

hops:
  - name: beacon-hop
    reload: 10s
    nodes: []
    http:
      url: "http://127.0.0.1:3000/api/stun/demo/gost/nodes?username=demo-user&password=pwd&serverName=home.example.com&caFile=%2Fpath%2Fto%2Fca.pem"
      timeout: 5s
```

启动方式：

```bash
gost -C gost.yaml
```

说明：

- `reload: 10s` 表示每 10 秒重新抓取一次最新地址。
- 只有新建连接会走新节点，已经建立的长连接不会自动迁移到新地址。
- `caFile` 必须是客户端本地可读路径，因此动态 URL 里需要传客户端自己的证书路径。
- 如果你继续使用命令行里的静态上游地址，那这个地址仍然不会跟随通道更新。

## Docker 运行

### 1. 本地构建镜像

```bash
docker build -t stunbeacon:local .
```

### 2. 运行容器

```bash
docker run --rm \
  -p 3000:3000 \
  -e AUTH_TOKEN=your-secret-token \
  -e LISTEN_ADDR=0.0.0.0:3000 \
  stunbeacon:local
```

## 触发 GitHub Release 构建

当你把带有 `v*` 前缀的 Tag 推送到 GitHub 时，`.github/workflows/release.yml` 会自动触发：

```bash
git tag v1.0.0
git push origin v1.0.0
```

工作流会自动构建以下二进制目标：

- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-musl`
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`

构建完成后：

- 压缩包会自动上传到对应的 GitHub Release 页面。
- Docker 镜像会自动推送到 `ghcr.io/jamebal/stunbeacon`。
- Docker 镜像只发布 Linux 多架构：
  - `linux/amd64`
  - `linux/arm64`
- GHCR 镜像不会在 Docker 构建阶段重新编译 Rust，而是直接复用前面产出的 Linux `musl` 二进制，以减少 QEMU/虚拟化带来的耗时。
- 每次发布标签会同时推送两个镜像标签：
  - `ghcr.io/jamebal/stunbeacon:vX.Y.Z`
  - `ghcr.io/jamebal/stunbeacon:latest`

### 拉取 GHCR 镜像

```bash
docker pull ghcr.io/jamebal/stunbeacon:v0.1.4
docker pull ghcr.io/jamebal/stunbeacon:latest
```

### 运行 GHCR 镜像

```bash
docker run --rm \
  -p 3000:3000 \
  -e AUTH_TOKEN=your-secret-token \
  ghcr.io/jamebal/stunbeacon:latest
```
