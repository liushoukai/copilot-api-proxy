# YApi 接口上传工具

将 Markdown 格式的 API 文档批量上传到 YApi 平台的命令行工具。

## 功能特性

- ✅ 自动解析 Markdown API 文档
- ✅ 自动跳过或更新已存在的接口
- ✅ 多种配置方式（环境变量/配置文件/命令行）
- ✅ 两步上传流程（创建接口 → 更新详情）
- ✅ JSON 自动生成 JSON Schema
- ✅ 遇错立即终止
- ✅ 支持自定义 YApi 服务域名

## 安装

```bash
# 进入工具目录
cd /Users/lvkun/Documents/Codes/browser_extension/tools/yapi-upload

# 创建虚拟环境并安装依赖
python3 -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
```

## 配置别名（推荐）

在 `~/.zshrc` 中添加别名，之后可直接使用 `yapi_upload` 命令：

```bash
# 编辑 ~/.zshrc，添加以下行
alias yapi_upload="/Users/lvkun/Documents/Codes/browser_extension/tools/yapi-upload/.venv/bin/python /Users/lvkun/Documents/Codes/browser_extension/tools/yapi-upload/yapi_upload.py"
```

然后执行：
```bash
source ~/.zshrc
```

## 获取 YApi Token 和 UID

1. 登录 YApi 平台（如 http://kapi.kugou.net）
2. 打开浏览器开发者工具（F12）
3. 切换到 **Application** 或 **存储** 标签
4. 查看 **Cookies** 中的：
   - `_yapi_token` → 用于 `--token` 参数
   - `_yapi_uid` → 用于 `--uid` 参数

## 配置方式

### 配置优先级（从高到低）

1. 命令行参数
2. 指定配置文件 `--config`
3. 项目目录配置 `.yapi.json`
4. 全局配置 `~/.yapi/config.json`
5. 环境变量
6. 默认值（仅 base_url）

### 方式 1: 命令行参数

```bash
yapi_upload ./api.md --project 100957 --catid 105866 --token xxx --uid 102452
```

### 方式 2: 环境变量

```bash
export YAPI_TOKEN="your_token_here"
export YAPI_UID="your_uid_here"
export YAPI_BASE_URL="http://yapi.example.com"  # 可选
```

### 方式 3: 全局配置文件

创建 `~/.yapi/config.json`：

```json
{
  "token": "your_token_here",
  "uid": "your_uid_here",
  "base_url": "http://kapi.kugou.net"
}
```

### 方式 4: 项目配置文件

在项目目录创建 `.yapi.json`：

```json
{
  "token": "your_token_here",
  "uid": "your_uid_here",
  "base_url": "http://kapi.kugou.net",
  "project_id": 100957
}
```

## 域名配置

默认使用 `http://kapi.kugou.net`，如需更换域名，可通过以下方式：

### 临时指定（命令行）

```bash
yapi_upload ./api.md --project 100957 --catid 105866 --base-url http://yapi.example.com
```

### 环境变量

```bash
export YAPI_BASE_URL="http://yapi.example.com"
yapi_upload ./api.md --project 100957 --catid 105866
```

### 配置文件

在 `~/.yapi/config.json` 或 `.yapi.json` 中设置：

```json
{
  "base_url": "http://yapi.example.com"
}
```

## 使用方法

### 基本用法

```bash
yapi_upload <markdown_file> --project <项目ID> --catid <分类ID>
```

### 完整示例

```bash
# 基本用法（使用默认域名，跳过已存在接口）
yapi_upload ./API_CURL_EXAMPLES.md --project 100957 --catid 105866

# 更新已存在的接口（请求参数和响应内容会更新）
yapi_upload ./API_CURL_EXAMPLES.md --project 100957 --catid 105866 --update

# 指定域名
yapi_upload ./API_CURL_EXAMPLES.md --project 100957 --catid 105866 \
  --base-url http://yapi.example.com

# 完整参数
yapi_upload ./API_CURL_EXAMPLES.md \
  --project 100957 \
  --catid 105866 \
  --token eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9... \
  --uid 102452 \
  --base-url http://kapi.kugou.net

# 测试解析（不上传）
yapi_upload ./API_CURL_EXAMPLES.md --project 100957 --catid 105866 --dry-run

# 详细输出
yapi_upload ./API_CURL_EXAMPLES.md --project 100957 --catid 105866 -v
```

## 命令行参数

| 参数 | 必需 | 说明 |
|------|:----:|------|
| `markdown_file` | ✅ | Markdown API 文档路径 |
| `--project` | ✅ | YApi 项目 ID |
| `--catid` | ✅ | 接口分类 ID |
| `--token` | | YApi token（可配置） |
| `--uid` | | YApi uid（可配置） |
| `--base-url` | | YApi 服务地址（默认: http://kapi.kugou.net） |
| `--config` | | 指定配置文件路径 |
| `--update` | | 更新已存在的接口（默认跳过已存在接口） |
| `--dry-run` | | 仅解析不上传，用于测试 |
| `--verbose, -v` | | 显示详细日志 |
| `--help, -h` | | 显示帮助信息 |

## Markdown 文档格式

工具支持以下格式的 Markdown 文档：

```markdown
### 1.1 接口标题

**接口**: `POST /v1/user/wish/do`

**请求方式**: POST (JSON Body)

**请求参数**:
| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| userid | int64 | 是 | 用户ID |
| celebrity_id | int64 | 是 | 艺人ID |

**curl 示例**:
```bash
curl -X POST "${BASE_URL}/v1/user/wish/do" \
  -d '{"userid": 12345, "celebrity_id": 67890}'
```

**响应示例**:
```json
{
  "errcode": 0,
  "errmsg": "success",
  "data": null
}
```
```

### 格式要求

1. **接口标题**: `### 序号 标题名称`
2. **接口定义**: `**接口**: \`METHOD /path\``
3. **参数表格**: 包含 `参数名`、`类型`、`必填`、`说明` 列
4. **响应示例**: JSON 格式的代码块

## 执行流程

```
1. 解析 Markdown 文件
      ↓
2. 提取所有接口信息（标题、路径、方法、参数、响应）
      ↓
3. 遍历每个接口：
   ├── 检查是否已存在
   ├── --update 模式：直接更新已存在的接口
   ├── 默认模式：跳过已存在接口
   ├── 调用 /api/interface/add 创建接口
   └── 调用 /api/interface/up 更新详情
      ↓
4. 输出汇总报告
```

## 输出示例

### 默认模式（跳过已存在）

```
正在解析: ./API_CURL_EXAMPLES.md

解析到 8 个接口:

  1. [POST] /v1/user/wish/do - 用户许愿（添加许愿记录）
  2. [POST] /v1/user/wish/cancel - 取消许愿
  ...

开始上传...
  处理: [POST] /v1/user/wish/do ... 跳过 (已存在)
  处理: [POST] /v1/user/wish/cancel ... 成功
  ...

==================================================
上传结果汇总
==================================================
  ○ [POST] /v1/user/wish/do
    接口已存在，跳过
  ✓ [POST] /v1/user/wish/cancel
  ...

成功: 6, 跳过: 2, 失败: 0
```

### 更新模式（--update）

```
正在解析: ./API_CURL_EXAMPLES.md

解析到 8 个接口:
  ...

开始上传...
  处理: [POST] /v1/user/wish/do ... 成功 (已更新)
  处理: [POST] /v1/user/wish/cancel ... 成功 (已更新)
  ...

==================================================
上传结果汇总
==================================================
  ✓ [POST] /v1/user/wish/do
  ✓ [POST] /v1/user/wish/cancel
  ...

成功: 8, 跳过: 0, 失败: 0
```

## 常见问题

### Q: Token 过期怎么办？

A: 重新登录 YApi 平台，获取新的 `_yapi_token` 和 `_yapi_uid`。

### Q: 如何更换 YApi 服务域名？

A: 使用 `--base-url` 参数或配置文件中的 `base_url` 字段。

### Q: 如何更新已存在的接口？

A: 添加 `--update` 参数，工具会更新已存在接口的请求参数和响应内容。

### Q: 默认模式和更新模式有什么区别？

A:
- **默认模式**：跳过已存在的接口，只上传新接口
- **更新模式** (`--update`)：更新已存在接口的请求参数和响应内容

### Q: 解析失败怎么办？

A: 使用 `--dry-run -v` 参数查看详细解析过程，检查 Markdown 格式是否符合要求。

### Q: 接口已存在但内容有变化，如何同步？

A: 使用 `--update` 参数重新上传，会更新接口的请求参数和响应 Schema。

## 文件结构

```
tools/yapi-upload/
├── yapi_upload.py       # 主入口 CLI
├── parser.py            # Markdown 解析器
├── yapi_client.py       # YApi API 客户端
├── config.py            # 配置管理（含默认域名）
├── schema_generator.py  # JSON Schema 生成器
├── requirements.txt     # Python 依赖
└── README.md            # 本文档
```

## 修改默认域名

如需永久修改默认域名，编辑 `config.py` 文件：

```python
# 修改这一行
DEFAULT_BASE_URL = "http://your-yapi-server.com"
```
