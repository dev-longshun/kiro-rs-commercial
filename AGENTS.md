# AGENTS.md — Docker 部署操作手册（给 AI 助手）

你是用户的运维助手。用户把本仓库交给你后，目标是：**在用户指定的服务器上，用预构建 Docker 镜像部署 kiro-rs-commercial，并验证服务可用**。

不要从源码编译，除非用户明确要求。优先使用镜像：

```
ghcr.io/dev-longshun/kiro-rs-commercial:latest
```

备用标签：`beta`（main 分支构建）。

---

## 开始前向用户确认

1. 服务器 SSH 信息（主机、用户、端口、密钥或密码方式）
2. 是否已安装 Docker；若未安装，是否允许执行官方安装脚本
3. 两个密钥（由用户提供或你安全生成后告知用户保存）：
   - `API_KEY`：客户端调用本服务用
   - `ADMIN_API_KEY`：管理后台登录用
4. 端口策略：默认只绑定 `127.0.0.1:8990`（推荐）；仅当用户明确要求公网访问时再改为 `0.0.0.0` / `8990:8990`
5. 部署目录：默认 `~/kiro-rs`

若用户未提供密钥，生成足够随机的字符串，部署完成后**明文告知用户并提醒妥善保存**。

---

## 执行步骤（按顺序）

### 1. 连接服务器

使用用户提供的 SSH 方式登录。后续命令均在目标服务器上执行。

### 2. 检查 Docker

```bash
docker --version
docker compose version
```

若未安装且用户已授权：

```bash
curl -fsSL https://get.docker.com | sh
# 如需非 root 运行 docker，按发行版将用户加入 docker 组后重新登录
```

### 3. 创建目录

```bash
mkdir -p ~/kiro-rs/data
cd ~/kiro-rs
```

### 4. 写入 docker-compose.yml

将下面内容写入 `~/kiro-rs/docker-compose.yml`，替换密钥占位符：

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
      - API_KEY=<用户的API密钥>
      - ADMIN_API_KEY=<用户的管理后台密钥>
    restart: unless-stopped
```

要点：

- 必须挂载 `./data:/app/config`，保证配置与凭据持久化
- 首次启动若无 `config.json`，容器入口脚本会用环境变量生成
- 首次启动若无 `credentials.json`，会创建 `[]`；凭据之后在 Admin 添加

可选：若用户更想用文件配置，可写入 `~/kiro-rs/data/config.json`：

```json
{
  "apiKey": "<用户的API密钥>",
  "host": "0.0.0.0",
  "port": 8990,
  "adminApiKey": "<用户的管理后台密钥>"
}
```

`host` 在容器内应为 `0.0.0.0`，否则外部映射可能连不上。

### 5. 拉取并启动

```bash
cd ~/kiro-rs
docker compose pull
docker compose up -d
```

### 6. 验证

```bash
docker compose logs --tail=100
docker compose ps
```

成功标志：

- 容器状态为 `running` / `Up`
- 日志出现类似：`启动 Anthropic API 端点: 0.0.0.0:8990`

本机探测（在服务器上）：

```bash
curl -sS -o /dev/null -w "%{http_code}\n" http://127.0.0.1:8990/admin
```

管理页应能返回 HTTP 状态码（通常为 200 或需登录的页面响应，而非连接失败）。

可选 API 探测（将密钥换成真实 `API_KEY`）：

```bash
curl -sS http://127.0.0.1:8990/v1/models \
  -H "x-api-key: <API_KEY>"
```

### 7. 告知用户如何使用

部署成功后向用户说明：

1. **管理后台**（添加 Kiro 凭据）：`http://127.0.0.1:8990/admin`  
   - 本机绑定：需在用户自己电脑上建立 SSH 隧道后访问：  
     `ssh -L 8990:127.0.0.1:8990 <user>@<host>`  
     然后打开 `http://localhost:8990/admin`  
   - 登录密钥：`ADMIN_API_KEY`
2. **API 基址**：`http://127.0.0.1:8990/v1`（同机客户端）或经隧道 / 反代后的地址  
   - 请求头：`x-api-key: <API_KEY>` 或 `Authorization: Bearer <API_KEY>`
3. **数据目录**：`~/kiro-rs/data`（勿删除）
4. **更新镜像**：

```bash
cd ~/kiro-rs && docker compose pull && docker compose up -d
```

5. **看日志 / 重启 / 停止**：

```bash
docker compose logs -f
docker compose restart
docker compose down
```

---

## 约束（必须遵守）

1. **不要**把密钥提交进 git，或贴到公开 issue/日志收集处
2. **不要**省略 `/app/config` 卷挂载
3. **不要**把 `credentials.json` 做成只读配置挂载（会阻止 Token 回写）
4. 默认**不要**把端口暴露到公网；用户明确要求时再改，并提醒防火墙与密钥强度
5. 优先 `docker compose`；仅在无 compose 时退回 `docker run`（见下方）
6. 拉取 `ghcr.io` 失败时：检查网络、DNS、是否需镜像加速或代理；不要擅自改用未知第三方镜像

### docker run 等价命令（无 compose 时）

```bash
mkdir -p ~/kiro-rs/data
docker run -d \
  --name kiro-rs \
  --restart unless-stopped \
  --add-host=host.docker.internal:host-gateway \
  -p 127.0.0.1:8990:8990 \
  -v ~/kiro-rs/data:/app/config \
  -e API_KEY="<API_KEY>" \
  -e ADMIN_API_KEY="<ADMIN_API_KEY>" \
  ghcr.io/dev-longshun/kiro-rs-commercial:latest
```

---

## 故障排查

| 现象 | 处理 |
|------|------|
| 拉镜像失败 | 检查外网/`ghcr.io` 访问；必要时配置 Docker 代理或镜像源 |
| 端口占用 | `ss -lntp \| grep 8990` 或 `lsof -i :8990`；换宿主机端口映射如 `127.0.0.1:8991:8990` |
| 容器反复退出 | `docker compose logs`；检查 `data` 目录权限、磁盘空间 |
| 管理后台打不开 | 确认绑定的是 `127.0.0.1` 时是否已建 SSH 隧道；确认容器 Up |
| 凭据重启后丢失 | 确认 `./data:/app/config` 已挂载且未把 credentials 只读覆盖 |
| API 401 | 核对请求头中的 key 是否等于 `API_KEY` / `config.json` 的 `apiKey` |

---

## 完成定义（Definition of Done）

同时满足即可向用户报告「部署完成」：

1. 容器持续运行
2. 日志中有服务监听 `0.0.0.0:8990`（或用户指定端口）
3. 已告知用户：`API_KEY`、`ADMIN_API_KEY`、Admin 地址、API 基址、数据目录与更新命令
4. 已说明：凭据需在 Admin 中添加；默认仅本机端口，远程需 SSH 隧道
