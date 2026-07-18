# kiro-rs-commercial

Anthropic Claude API 兼容代理服务。本仓库文档只说明如何用 **Docker 镜像** 部署。

## 镜像

```
ghcr.io/dev-longshun/kiro-rs-commercial:latest
```

标签：

- `latest` — 正式版本（打 `v*` tag 时更新）
- `beta` — 跟随 `main` 分支构建

## 快速部署

### 前置

- 已安装 Docker（及可选的 Docker Compose）
- 服务器可访问 `ghcr.io` 拉取镜像

安装 Docker（如未安装）：

```bash
curl -fsSL https://get.docker.com | sh
```

### 方式一：docker run

```bash
mkdir -p ~/kiro-rs/data

docker run -d \
  --name kiro-rs \
  --restart unless-stopped \
  -p 127.0.0.1:8990:8990 \
  -v ~/kiro-rs/data:/app/config \
  -e API_KEY="你的API密钥" \
  -e ADMIN_API_KEY="你的管理后台密钥" \
  ghcr.io/dev-longshun/kiro-rs-commercial:latest
```

说明：

- 首次启动且 `/app/config` 下没有 `config.json` 时，镜像会用环境变量生成配置
- 没有 `credentials.json` 时会创建空数组 `[]`，凭据请之后在管理后台添加
- 必须挂载 `/app/config`，否则容器重建后凭据会丢失

### 方式二：Docker Compose

```bash
mkdir -p ~/kiro-rs/data
cd ~/kiro-rs
```

创建 `docker-compose.yml`：

```yaml
services:
  kiro-rs:
    image: ghcr.io/dev-longshun/kiro-rs-commercial:latest
    container_name: kiro-rs
    extra_hosts:
      - "host.docker.internal:host-gateway"
    ports:
      - "127.0.0.1:8990:8990"
    volumes:
      - ./data:/app/config
    environment:
      - API_KEY=你的API密钥
      - ADMIN_API_KEY=你的管理后台密钥
    restart: unless-stopped
```

启动：

```bash
docker compose pull
docker compose up -d
docker compose logs -f
```

日志中出现 `启动 Anthropic API 端点: 0.0.0.0:8990` 即表示成功。

也可在挂载目录中预先放置 `config.json`（此时可不依赖环境变量）：

```json
{
  "apiKey": "你的API密钥",
  "host": "0.0.0.0",
  "port": 8990,
  "adminApiKey": "你的管理后台密钥"
}
```

路径：`./data/config.json` → 容器内 `/app/config/config.json`。

## 访问

默认端口映射为 `127.0.0.1:8990`，仅本机可访问。

| 用途 | 地址 |
|------|------|
| 管理后台 | `http://127.0.0.1:8990/admin` |
| Anthropic 兼容 API | `http://127.0.0.1:8990/v1` |

客户端请求需带上 `apiKey`，例如：

```
x-api-key: 你的API密钥
```

或：

```
Authorization: Bearer 你的API密钥
```

远程服务器上可用 SSH 隧道在本地打开管理后台：

```bash
ssh -L 8990:127.0.0.1:8990 user@服务器IP
```

然后在浏览器访问 `http://localhost:8990/admin`，用 `ADMIN_API_KEY` 登录并添加凭据。

如需公网直连，将端口改为 `8990:8990`，并自行做好鉴权与防火墙。

## 环境变量

镜像入口与运行时会读取以下变量（存在时覆盖 / 用于生成默认配置）：

| 变量 | 说明 |
|------|------|
| `API_KEY` | 客户端 API 密钥（对应 `apiKey`） |
| `ADMIN_API_KEY` | 管理后台密钥（对应 `adminApiKey`） |
| `HOST` | 监听地址，默认 `0.0.0.0` |
| `PORT` | 监听端口，默认 `8990` |
| `REGION` | AWS 区域 |
| `PROXY_URL` | 全局 HTTP/SOCKS5 代理 |
| `PROXY_USERNAME` | 代理用户名 |
| `PROXY_PASSWORD` | 代理密码 |

## 持久化

| 宿主机（示例） | 容器内 | 用途 |
|----------------|--------|------|
| `./data` 或 `~/kiro-rs/data` | `/app/config` | `config.json`、`credentials.json` 等 |

请勿把可写的 `credentials.json` 做成只读挂载（例如部分平台的 Config File），否则 Token 刷新与凭据写入会失败。

## 常用命令

```bash
# 日志
docker compose logs -f
# 或
docker logs -f kiro-rs

# 重启
docker compose restart

# 更新镜像
docker compose pull && docker compose up -d

# 停止
docker compose down
```

## 给 AI 助手（一键部署提示词）

把本仓库地址和下面这段话一起发给你的 AI 助手即可（按需补上 SSH 信息）：

```
请阅读本仓库根目录的 AGENTS.md，并严格按其中步骤，在我的服务器上用 Docker 镜像部署 kiro-rs-commercial。部署前向我确认 SSH 登录方式、API_KEY 与 ADMIN_API_KEY（若我未提供则安全生成并告知我），默认端口仅绑定 127.0.0.1:8990，完成后验证服务可用并说明如何访问管理后台与 API。
```

仓库地址示例：`https://github.com/dev-longshun/kiro-rs-commercial`

更细的操作约束与排查说明见 [AGENTS.md](./AGENTS.md)。

## License

MIT

## 致谢

本项目基于 [kiro.rs](https://github.com/hank9999/kiro.rs) 二次开发，感谢原作者的开源贡献。
