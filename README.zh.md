# copilot-api-proxy

[English](./README.md) | 中文

将 GitHub Copilot 包装为兼容 **OpenAI / Anthropic** 接口的本地代理服务。  
只需拥有 GitHub Copilot 订阅，无需额外 API Key。

---

## 快速开始

### 第一步：授权登录

```bash
./copilot-api-proxy auth
```

> 首次使用需要执行。通过 GitHub Device Flow 完成授权，Token 会缓存到本地。

### 第二步：启动代理服务

```bash
./copilot-api-proxy start
```

启动成功后，代理监听在 **`http://127.0.0.1:4142`**。

### 第三步：配置 Claude Code

打开 Claude Code 的 `settings.json`，添加以下内容：

```json
{
  "env": {
    "ANTHROPIC_BASE_URL": "http://localhost:4142"
  }
}
```

> **`settings.json` 在哪里？**  
> macOS：`~/Library/Application Support/Claude/claude_code/settings.json`  
> Linux：`~/.config/Claude/claude_code/settings.json`

配置完成后，启动 Claude Code，所有 Anthropic API 请求都会通过你的 Copilot 订阅转发。

---

## 从源码构建

```bash
cargo build --release
# 产物路径：./target/release/copilot-api-proxy
```

---

## 命令参数

### `auth`

```bash
./copilot-api-proxy auth [flags]
```

| 参数 | 说明 |
|------|------|
| `-f, --force` | 强制重新授权，忽略本地缓存的 Token |
| `--show-token` | 授权成功后在终端打印 GitHub Token |

### `start`

```bash
./copilot-api-proxy start [flags]
```

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `-p, --port` | `4142` | 监听端口 |
| `--host` | `127.0.0.1` | 监听地址。指定 `0.0.0.0` 可暴露到局域网（注意安全风险） |
| `-v, --verbose` | `false` | 开启 DEBUG 级别详细日志 |
| `-g, --github-token` | — | 直接传入 GitHub Token，跳过授权流程 |
| `-a, --account-type` | `individual` | 账户类型：`individual` / `business` / `enterprise` |
| `--show-token` | `false` | 启动时在终端打印 GitHub Token 和 Copilot Token |
| `--proxy` | — | HTTP/HTTPS 代理地址，例如 `http://127.0.0.1:7890`。等效于同时设置 `HTTP_PROXY` 和 `HTTPS_PROXY` 环境变量。中国大陆访问 Claude 模型时必须设置。 |

---

## 功能特性

- 兼容 OpenAI Chat Completions API（`/v1/chat/completions`）
- 兼容 Anthropic Messages API（`/v1/messages`），支持流式响应
- 兼容 OpenAI Embeddings API（`/v1/embeddings`）
- 支持模型列表查询（`/v1/models`）
- 支持 HTTP/HTTPS 代理透传

## API 端点

| 端点 | 说明 |
|------|------|
| `GET /health` | 健康检查，返回 Token 状态 |
| `GET /v1/models` | 获取可用模型列表 |
| `POST /v1/chat/completions` | OpenAI Chat Completions（支持流式） |
| `POST /v1/embeddings` | OpenAI Embeddings |
| `POST /v1/messages` | Anthropic Messages API（支持流式） |

---

## 更多示例

### 配合代理启动（中国大陆使用 Claude 模型必须）

GitHub Copilot 会根据请求的出口 IP 返回不同的模型列表。从中国大陆直连时，服务端会因 Anthropic 的地区限制过滤掉所有 `claude-*` 模型；走海外代理后，才能拿到完整列表。

```bash
./copilot-api-proxy start --port 4142 --proxy http://127.0.0.1:7890
```

如果启动日志中出现以下警告：

```
WARN ⚠️  模型列表中没有 claude-* 模型 ...
```

说明当前出口 IP 位于中国大陆，Copilot 服务端已过滤 Claude 模型。请加上 `--proxy` 参数后重新启动。

### 使用 PM2 作为守护进程运行

[PM2](https://pm2.keymetrics.io/) 是跨平台进程管理器，支持后台运行、崩溃自动重启和开机自启。

**安装 PM2（一次性）：**

```bash
npm install -g pm2
```

**方式一 — 命令行启动（快速）：**

```bash
pm2 start ./copilot-api-proxy \
  --name copilot-api-proxy \
  --restart-delay 3000 \
  --max-restarts 10 \
  -- start --port 4142 --proxy http://127.0.0.1:7890
```

**方式二 — 配置文件启动（推荐长期使用）：**

在二进制文件同目录下创建 `ecosystem.config.js`：

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

然后启动：

```bash
pm2 start ecosystem.config.js
```

**注册开机自启（两种方式通用）：**

```bash
pm2 save
pm2 startup   # 按照输出的提示执行一条命令，即可实现登录后自动启动
```

**常用命令：**

```bash
pm2 status                  # 查看运行状态
pm2 logs copilot-api-proxy  # 查看日志
pm2 restart copilot-api-proxy
pm2 stop copilot-api-proxy
pm2 delete copilot-api-proxy
```

> **注意：** `pm2 status` 的版本列显示 `N/A`，这是正常现象。PM2 从 `package.json` 读取版本号，Rust 二进制没有该文件，不影响任何功能。



```bash
./copilot-api-proxy start --github-token ghp_xxxxxxxxxxxx
```

### Claude Code 单次临时使用（不修改配置文件）

```bash
ANTHROPIC_BASE_URL=http://127.0.0.1:4142 claude
```

### 配置到 OpenAI 兼容客户端

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

### curl 测试

```bash
curl http://127.0.0.1:4142/v1/messages \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-3.5-sonnet",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```
