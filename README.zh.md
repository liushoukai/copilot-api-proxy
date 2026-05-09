# copilot-api-proxy

[English](./README.md) | 中文

将 GitHub Copilot 包装为兼容 **OpenAI / Anthropic** 接口的本地代理服务。  
只需拥有 GitHub Copilot 订阅，即可通过标准 API 调用各类 Copilot 模型。

## 功能特性

- 兼容 OpenAI Chat Completions API（`/v1/chat/completions`）
- 兼容 Anthropic Messages API（`/v1/messages`），支持流式响应
- 兼容 OpenAI Embeddings API（`/v1/embeddings`）
- 支持模型列表查询（`/v1/models`）
- 支持 HTTP/HTTPS 代理透传

## 构建

```bash
cargo build --release
```

编译产物位于 `./target/release/copilot-api-proxy`。

## 使用流程

### 第一步：授权登录

首次使用需通过 GitHub Device Flow 完成授权，Token 会缓存到本地：

```bash
./target/release/copilot-api-proxy auth
```

| 参数 | 说明 |
|------|------|
| `-f, --force` | 强制重新授权，忽略本地缓存的 Token |
| `--show-token` | 授权成功后在终端打印 GitHub Token |

### 第二步：启动代理服务

```bash
./target/release/copilot-api-proxy start
```

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `-p, --port` | `4142` | 监听端口 |
| `-v, --verbose` | `false` | 开启 DEBUG 级别详细日志 |
| `-g, --github-token` | — | 直接传入 GitHub Token，跳过授权流程 |
| `-a, --account-type` | `individual` | 账户类型：`individual` / `business` / `enterprise` |
| `--show-token` | `false` | 启动时在终端打印 GitHub Token 和 Copilot Token |

启动成功后，代理监听在 `http://127.0.0.1:4142`。

## API 端点

| 端点 | 说明 |
|------|------|
| `GET /health` | 健康检查，返回 Token 状态 |
| `GET /v1/models` | 获取可用模型列表 |
| `POST /v1/chat/completions` | OpenAI Chat Completions（支持流式） |
| `POST /v1/embeddings` | OpenAI Embeddings |
| `POST /v1/messages` | Anthropic Messages API（支持流式） |

## 示例

### 配合代理启动（需要科学上网时）

```bash
HTTP_PROXY=http://127.0.0.1:7890 HTTPS_PROXY=http://127.0.0.1:7890 \
  ./target/release/copilot-api-proxy start --port 4142 --verbose
```

### 直接传入 GitHub Token 启动

```bash
./target/release/copilot-api-proxy start --github-token ghp_xxxxxxxxxxxx
```

### 调用 Chat Completions

```bash
curl http://127.0.0.1:4142/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4o",
    "messages": [{"role": "user", "content": "Hello!"}],
    "stream": false
  }'
```

### 调用 Anthropic Messages API

```bash
curl http://127.0.0.1:4142/v1/messages \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3.5-sonnet",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

### 配置到 OpenAI 兼容客户端

将客户端的 `base_url` 设置为 `http://127.0.0.1:4142/v1`，`api_key` 填任意字符串即可。

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
