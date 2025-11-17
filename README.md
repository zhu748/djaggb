# ClewdR

<p align="center">
  <img src="./assets/clewdr-logo.svg" alt="ClewdR" height="60">
</p>

ClewdR is a Rust proxy for Claude (Claude.ai, Claude Code) and Google Gemini (AI Studio, Vertex AI).  
It keeps resource usage low, serves OpenAI-style endpoints, and ships with a small React admin UI for managing cookies, keys, and settings.

---

## Highlights

- Works with Claude web, Claude Code, Gemini AI Studio, and Vertex AI.
- Single static binary for Linux, macOS, Windows, and Android; Docker image available.
- Web dashboard shows live status, exposes cookie/key editors, and supports hot config reloads.
- Drops into existing OpenAI-compatible clients while keeping native Claude/Gemini formats.
- Optional SQLite/Postgres/MySQL persistence; default `clewdr.toml` file mode still available.
- Typical production footprint: `<10 MB` RAM, `<1 s` startup, `~15 MB` binary.

## Supported Endpoints

| Service | Endpoint |
|---------|----------|
| Claude native | `http://127.0.0.1:8484/v1/messages` |
| Claude OpenAI compatible | `http://127.0.0.1:8484/v1/chat/completions` |
| Claude Code | `http://127.0.0.1:8484/code/v1/messages` |
| Gemini native | `http://127.0.0.1:8484/v1/v1beta/generateContent` |
| Gemini OpenAI compatible | `http://127.0.0.1:8484/gemini/chat/completions` |
| Vertex AI proxy | `http://127.0.0.1:8484/v1/vertex/v1beta/` |

Streaming responses work on every endpoint.

## Quick Start

1. Download the latest release for your platform from GitHub.  
   Linux/macOS example:
   ```bash
   curl -L -o clewdr.tar.gz https://github.com/Xerxes-2/clewdr/releases/latest/download/clewdr-linux-x64.tar.gz
   tar -xzf clewdr.tar.gz && cd clewdr-linux-x64
   chmod +x clewdr
   ```
2. Run the binary:
   ```bash
   ./clewdr
   ```
3. Open `http://127.0.0.1:8484` and enter the admin password shown in the console (or container logs if using Docker).

## Using the Web Admin

- `Dashboard` shows health, connected clients, and rate-limit status.
- `Claude` tab stores browser cookies; paste `cookie: value` pairs and save.
- `Gemini` tab accepts AI Studio keys and optional Vertex OAuth credentials.
- `Settings` lets you rotate the admin password, set upstream proxies, and reload config without restarting.

If you forget the password, delete `clewdr.toml` and start the binary again. Docker users can mount a persistent folder for that file.

## Configure Upstreams

### Claude

1. Export your Claude.ai cookies (e.g., via browser devtools).  
2. Paste them into the Claude tab; ClewdR tracks their status automatically.  
3. Optionally set an outbound proxy or fingerprint overrides if Claude blocks your region.

### Gemini

1. Add AI Studio API keys in the Gemini tab.  
2. For Vertex AI, provide OAuth client ID/secret and project info.  
3. Select default models or override them per request via the API.

## Client Examples

SillyTavern:

```json
{
  "api_url": "http://127.0.0.1:8484/v1/chat/completions",
  "api_key": "password-from-console",
  "model": "claude-3-sonnet-20240229"
}
```

Continue (VS Code):

```json
{
  "models": [
    {
      "title": "Claude via ClewdR",
      "provider": "openai",
      "model": "claude-3-sonnet-20240229",
      "apiBase": "http://127.0.0.1:8484/v1/",
      "apiKey": "password-from-console"
    }
  ]
}
```

Cursor:

```json
{
  "openaiApiBase": "http://127.0.0.1:8484/v1/",
  "openaiApiKey": "password-from-console"
}
```

## Persistence Options

ClewdR stores everything in `clewdr.toml` by default. To use a database instead, build with the matching feature flag and set `persistence.mode`.

### Build with database features

```bash
cargo build --release --no-default-features --features "embed-resource,xdg,db-sqlite"
```

Available feature flags: `db-sqlite`, `db-postgres`, `db-mysql` (each pulls in the core `db` feature).  
Custom Docker builds should pass the same flags during `cargo build`.

### Configure the backend

`clewdr.toml` example:

```toml
[persistence]
mode = "postgres"                           # sqlite | postgres | mysql
database_url = "postgres://user:pass@db:5432/clewdr"
```

- SQLite: optionally set `sqlite_path = "/var/lib/clewdr/clewdr.db"`. ClewdR expands it to `sqlite:///...` and creates the parent folder if possible.
- Postgres/MySQL: `database_url` is required.
- Environment variables use Figment’s double underscore style, for example:

  ```bash
  export CLEWDR_PERSISTENCE__MODE=sqlite
  export CLEWDR_PERSISTENCE__SQLITE_PATH=/var/lib/clewdr/clewdr.db
  export CLEWDR_PERSISTENCE__DATABASE_URL="postgres://user:pass@db/clewdr"
  ```

Operational notes:

- On first run ClewdR applies SeaORM migrations for `config`, `cookies`, `keys`, and `wasted` tables.
- `GET /api/storage/status` reports database health; writes fail if storage is unavailable.
- Verify the running binary exposes DB support (`clewdr -V`) when switching from file mode.

## Resources

- Wiki: <https://github.com/Xerxes-2/clewdr/wiki>  
  - Database persistence guide (中文): `wiki/database.md`

## Thanks

- [wreq](https://github.com/0x676e67/wreq) for the fingerprinting library.  
- [Clewd](https://github.com/teralomaniac/clewd) for many upstream ideas.  
- [Clove](https://github.com/mirrorange/clove) for Claude Code helpers.
