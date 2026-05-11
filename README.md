# copilot-api-proxy

English | [中文](./README.zh.md)

A local proxy that wraps GitHub Copilot as a standard **OpenAI / Anthropic**-compatible API.  
All you need is a GitHub Copilot subscription — no extra API key required.

---

## Quick Start

### 1. Authenticate

```bash
./copilot-api-proxy auth
```

> First run only. Authorizes via GitHub Device Flow and caches the token locally.

### 2. Start the proxy

```bash
./copilot-api-proxy start
```

The proxy is now running at **`http://127.0.0.1:4142`**.

### 3. Configure Claude Code

Open your Claude Code `settings.json` and add:

```json
{
  "env": {
    "ANTHROPIC_BASE_URL": "http://localhost:4142"
  }
}
```

> **Where is `settings.json`?**  
> macOS: `~/Library/Application Support/Claude/claude_code/settings.json`  
> Linux: `~/.config/Claude/claude_code/settings.json`

That's it. Start Claude Code and all Anthropic API requests will be routed through your Copilot subscription.

---

## Build from Source

```bash
cargo build --release
# Binary: ./target/release/copilot-api-proxy
```

---

## CLI Reference

### `auth`

```bash
./copilot-api-proxy auth [flags]
```

| Flag | Description |
|------|-------------|
| `-f, --force` | Force re-authentication, ignoring cached token |
| `--show-token` | Print the GitHub Token after auth |

### `start`

```bash
./copilot-api-proxy start [flags]
```

| Flag | Default | Description |
|------|---------|-------------|
| `-p, --port` | `4142` | Listening port |
| `--host` | `127.0.0.1` | Listening address. Use `0.0.0.0` to expose to LAN (use with caution) |
| `-v, --verbose` | `false` | Enable DEBUG level logging |
| `-g, --github-token` | — | Provide a GitHub Token directly, skip auth flow |
| `-a, --account-type` | `individual` | `individual` / `business` / `enterprise` |
| `--show-token` | `false` | Print GitHub Token and Copilot Token on startup |
| `--proxy` | — | HTTP/HTTPS proxy URL, e.g. `http://127.0.0.1:7890`. Equivalent to setting `HTTP_PROXY` and `HTTPS_PROXY` env vars. Required in mainland China to access Claude models. |

---

## Features

- OpenAI Chat Completions API (`/v1/chat/completions`)
- Anthropic Messages API (`/v1/messages`) with streaming
- OpenAI Embeddings API (`/v1/embeddings`)
- Model listing (`/v1/models`)
- HTTP/HTTPS proxy passthrough via environment variables

## API Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /health` | Health check, returns token status |
| `GET /v1/models` | List available models |
| `POST /v1/chat/completions` | OpenAI Chat Completions (streaming supported) |
| `POST /v1/embeddings` | OpenAI Embeddings |
| `POST /v1/messages` | Anthropic Messages API (streaming supported) |

---

## More Examples

### Start with an HTTP proxy (required outside mainland China for Claude models)

GitHub Copilot filters available models based on your exit IP. When connecting directly from mainland China, the server omits all `claude-*` models due to Anthropic's regional restrictions. Routing through an overseas proxy restores the full model list.

```bash
./copilot-api-proxy start --port 4142 --proxy http://127.0.0.1:7890
```

If you start without a proxy and see this warning in the logs:

```
WARN ⚠️  模型列表中没有 claude-* 模型 ...
```

it means your current exit IP is in mainland China and Copilot has filtered out Claude models. Add `--proxy` and restart.

### Run as a daemon with PM2

[PM2](https://pm2.keymetrics.io/) is a cross-platform process manager that keeps the proxy running in the background, restarts it on crash, and can survive reboots.

**Install PM2 (once):**

```bash
npm install -g pm2
```

**Option 1 — command line (quick start):**

```bash
pm2 start ./copilot-api-proxy \
  --name copilot-api-proxy \
  --restart-delay 3000 \
  --max-restarts 10 \
  -- start --port 4142 --proxy http://127.0.0.1:7890
```

**Option 2 — config file (recommended for permanent setup):**

Create `ecosystem.config.js` in the same directory as the binary:

```js
module.exports = {
  apps: [
    {
      name: 'copilot-api-proxy',
      script: './copilot-api-proxy',
      args: 'start --port 4142 --proxy http://127.0.0.1:7890',
      restart_delay: 3000,
      max_restarts: 10,
    },
  ],
};
```

Then start with:

```bash
pm2 start ecosystem.config.js
```

**Register for auto-start on login (both options):**

```bash
pm2 save
pm2 startup   # follow the printed instruction to enable boot persistence
```

**Common commands:**

```bash
pm2 status                  # check running status
pm2 logs copilot-api-proxy  # tail logs
pm2 restart copilot-api-proxy
pm2 stop copilot-api-proxy
pm2 delete copilot-api-proxy
```

> **Note:** `pm2 status` shows `N/A` for the version column. This is normal — PM2 reads version from `package.json`, which doesn't exist for a Rust binary. It has no effect on functionality.



```bash
./copilot-api-proxy start --github-token ghp_xxxxxxxxxxxx
```

### Use Claude Code with a one-off environment variable

```bash
ANTHROPIC_BASE_URL=http://127.0.0.1:4142 claude
```

### Use with an OpenAI-compatible client

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:4142/v1",
    api_key="unused",
)

response = client.chat.completions.create(
    model="gpt-4o",
    messages=[{"role": "user", "content": "Hello!"}],
)
print(response.choices[0].message.content)
```

### Test with curl

```bash
curl http://127.0.0.1:4142/v1/messages \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3.5-sonnet",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```
