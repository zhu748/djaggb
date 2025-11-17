# ClewdR

<p align="center">
  <img src="./assets/clewdr-logo.svg" alt="ClewdR" height="60">
</p>

ClewdR 是面向 Claude（Claude.ai、Claude Code）和 Google Gemini（AI Studio、Vertex AI）的 Rust 代理。  
它提供低资源占用的多端点转发，并附带一个 React 管理界面用于管理 Cookie、密钥和配置。

---

## 核心特点

- 对接 Claude Web、Claude Code、Gemini AI Studio、Vertex AI。
- 单个静态二进制可运行在 Linux、macOS、Windows、Android，另有 Docker 镜像。
- 网页控制台可查看状态、编辑 Cookie/Key，并支持热加载配置。
- 同时支持 OpenAI 兼容接口和原生 Claude/Gemini 协议，流式响应可用。
- 默认使用本地 `clewdr.toml`，也可选择 SQLite/Postgres/MySQL。
- 典型占用：`<10 MB` 内存、`<1 秒` 启动、`~15 MB` 二进制。

## 支持的端点

| 服务 | 地址 |
|------|------|
| Claude 原生 | `http://127.0.0.1:8484/v1/messages` |
| Claude OpenAI 兼容 | `http://127.0.0.1:8484/v1/chat/completions` |
| Claude Code | `http://127.0.0.1:8484/code/v1/messages` |
| Gemini 原生 | `http://127.0.0.1:8484/v1/v1beta/generateContent` |
| Gemini OpenAI 兼容 | `http://127.0.0.1:8484/gemini/chat/completions` |
| Vertex AI 代理 | `http://127.0.0.1:8484/v1/vertex/v1beta/` |

所有端点均支持流式返回。

## 快速开始

1. 从 GitHub Releases 下载对应平台的最新版。  
   Linux/macOS 示例：
   ```bash
   curl -L -o clewdr.tar.gz https://github.com/Xerxes-2/clewdr/releases/latest/download/clewdr-linux-x64.tar.gz
   tar -xzf clewdr.tar.gz && cd clewdr-linux-x64
   chmod +x clewdr
   ```
2. 运行二进制：
   ```bash
   ./clewdr
   ```
3. 打开 `http://127.0.0.1:8484`，使用控制台（或 Docker 容器日志）显示的管理员密码登录。

## Web 管理界面

- `Dashboard`：查看健康状态、限流命中、连接数。
- `Claude`：粘贴浏览器导出的 Cookie，ClewdR 自动检测有效性。
- `Gemini`：录入 AI Studio 密钥，可选配置 Vertex OAuth。
- `Settings`：修改管理员密码、上游代理、指纹配置，支持热重载。

如忘记密码，删除 `clewdr.toml` 再启动即可。Docker 建议挂载该文件所在目录以持久化。

## 配置上游

### Claude

1. 在浏览器开发者工具导出 Claude.ai Cookie。  
2. 粘贴至 Claude 页签并保存，ClewdR 会实时标记状态。  
3. 如需自定义网络出口，可设置上游代理或指纹选项。

### Gemini

1. 在 Gemini 页签添加 AI Studio API Key。  
2. 若使用 Vertex AI，填写 OAuth Client 信息与项目参数。  
3. 可在界面或请求中指定默认模型。

## 客户端示例

SillyTavern：

```json
{
  "api_url": "http://127.0.0.1:8484/v1/chat/completions",
  "api_key": "控制台显示的密码",
  "model": "claude-3-sonnet-20240229"
}
```

Continue（VS Code）：

```json
{
  "models": [
    {
      "title": "Claude via ClewdR",
      "provider": "openai",
      "model": "claude-3-sonnet-20240229",
      "apiBase": "http://127.0.0.1:8484/v1/",
      "apiKey": "控制台显示的密码"
    }
  ]
}
```

Cursor：

```json
{
  "openaiApiBase": "http://127.0.0.1:8484/v1/",
  "openaiApiKey": "控制台显示的密码"
}
```

## 持久化选项

默认使用 `clewdr.toml`。若需数据库，请在构建时启用匹配特性并配置 `persistence.mode`。

### 构建包含数据库支持的二进制

```bash
cargo build --release --no-default-features --features "embed-resource,xdg,db-sqlite"
```

可选特性：`db-sqlite`、`db-postgres`、`db-mysql`（均会启用基础 `db` 特性）。  
自定义 Docker 镜像需在 `cargo build` 步骤中加入同样的特性。

### 配置示例

`clewdr.toml`：

```toml
[persistence]
mode = "postgres"                           # sqlite | postgres | mysql
database_url = "postgres://user:pass@db:5432/clewdr"
```

- SQLite 可设置 `sqlite_path = "/var/lib/clewdr/clewdr.db"`，ClewdR 会自动扩展为 `sqlite:///...` 并尝试创建目录。
- Postgres/MySQL 需要提供 `database_url`。
- 环境变量使用 Figment 的双下划线形式，例如：

  ```bash
  export CLEWDR_PERSISTENCE__MODE=sqlite
  export CLEWDR_PERSISTENCE__SQLITE_PATH=/var/lib/clewdr/clewdr.db
  export CLEWDR_PERSISTENCE__DATABASE_URL="postgres://user:pass@db/clewdr"
  ```

运行提示：

- 首次启动会执行 SeaORM 迁移，创建 `config`、`cookies`、`keys`、`wasted` 表。
- `GET /api/storage/status` 可检查存储健康状态；若数据库不可用，写入接口会直接失败。
- 切换到数据库模式前，请确认二进制已启用对应特性（`clewdr -V` 可查看）。

## 资源

- Wiki：<https://github.com/Xerxes-2/clewdr/wiki>  
  - 数据库持久化指南（中文）：`wiki/database.md`

## 致谢

- [wreq](https://github.com/0x676e67/wreq) 提供指纹识别能力。  
- [Clewd](https://github.com/teralomaniac/clewd) 提供参考实现。  
- [Clove](https://github.com/mirrorange/clove) 提供 Claude Code 相关逻辑。
