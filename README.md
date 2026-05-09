# copilot-api-proxy

English | [中文](./README.zh.md)

A local proxy that wraps GitHub Copilot as a standard **OpenAI / Anthropic**-compatible API.  
All you need is a GitHub Copilot subscription to call Copilot models via standard API clients.

## Features

- OpenAI Chat Completions API compatible (`/v1/chat/completions`)
- Anthropic Messages API compatible (`/v1/messages`), with streaming support
- OpenAI Embeddings API compatible (`/v1/embeddings`)
- Model listing (`/v1/models`)
- HTTP/HTTPS proxy passthrough via environment variables

## Build

```bash
cargo build --release
```

The binary will be at `./target/release/copilot-api-proxy`.

## Usage

### Step 1: Authenticate

On first use, authorize via GitHub Device Flow. The token will be cached locally:

```bash
./target/release/copilot-api-proxy auth
```

| Flag | Description |
|------|-------------|
| `-f, --force` | Force re-authentication, ignoring cached token |
| `--show-token` | Print the GitHub Token to terminal after auth |

### Step 2: Start the proxy

```bash
./target/release/copilot-api-proxy start
```

| Flag | Default | Description |
|------|---------|-------------|
| `-p, --port` | `4142` | Listening port |
| `-v, --verbose` | `false` | Enable DEBUG level logging |
| `-g, --github-token` | — | Provide a GitHub Token directly, skip auth flow |
| `-a, --account-type` | `individual` | Account type: `individual` / `business` / `enterprise` |
| `--show-token` | `false` | Print GitHub Token and Copilot Token on startup |

Once started, the proxy listens at `http://127.0.0.1:4142`.

## API Endpoints

| Endpoint | Description |
|----------|-------------|
| `GET /health` | Health check, returns token status |
| `GET /v1/models` | List available models |
| `POST /v1/chat/completions` | OpenAI Chat Completions (streaming supported) |
| `POST /v1/embeddings` | OpenAI Embeddings |
| `POST /v1/messages` | Anthropic Messages API (streaming supported) |

## Examples

### Start with an HTTP proxy

```bash
HTTP_PROXY=http://127.0.0.1:7890 HTTPS_PROXY=http://127.0.0.1:7890 \
  ./target/release/copilot-api-proxy start --port 4142 --verbose
```

### Start with a GitHub Token directly

```bash
./target/release/copilot-api-proxy start --github-token ghp_xxxxxxxxxxxx
```

### Call Chat Completions

```bash
curl http://127.0.0.1:4142/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4o",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": false
  }'
```

### Call Anthropic Messages API

```bash
curl http://127.0.0.1:4142/v1/messages \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3.5-sonnet",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

### Use with an OpenAI-compatible client

Set `base_url` to `http://127.0.0.1:4142/v1` and use any string as `api_key`.

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
