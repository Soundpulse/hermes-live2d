# ATRI Live2D API

ATRI 桌宠对外 HTTP 接口，供 LLM skills、Python 脚本等外部程序调用。

**Base URL:** `http://127.0.0.1:3210`

**端口配置:** 环境变量 `ATRI_API_PORT`（默认 `3210`）

---

## 通用响应格式

```json
{
  "ok": true,
  "message": "描述信息"
}
```

错误时返回 HTTP 400：
```json
{
  "ok": false,
  "message": "错误原因"
}
```

---

## 接口列表

### GET /status

检查 API 服务是否运行。

**请求示例:**
```bash
curl http://127.0.0.1:3210/status
```

**响应:**
```json
{"ok": true, "message": "ATRI Live2D API is running"}
```

---

### GET /expressions

获取所有可用表情列表。

**请求示例:**
```bash
curl http://127.0.0.1:3210/expressions
```

**响应:**
```json
[
  {"id": 1, "name": "害羞"},
  {"id": 2, "name": "失去高光"},
  {"id": 3, "name": "吊带睡衣"},
  {"id": 4, "name": "内衣"},
  {"id": 5, "name": "穿凉鞋"},
  {"id": 6, "name": "穿皮鞋"},
  {"id": 7, "name": "愣住"},
  {"id": 8, "name": "白框"},
  {"id": 9, "name": "染血"},
  {"id": 10, "name": "小鸟"},
  {"id": 11, "name": "螃蟹"},
  {"id": 12, "name": "NO"},
  {"id": 13, "name": "YES"},
  {"id": 14, "name": "睡衣2"},
  {"id": 15, "name": "阴影"},
  {"id": 16, "name": "exp_16"},
  {"id": 17, "name": "exp_17"},
  {"id": 18, "name": "exp_18"},
  {"id": 19, "name": "exp_19"}
]
```

---

### POST /expression

切换 ATRI 的表情。支持按 ID 或名称指定。

**请求体:**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | number | 二选一 | 表情 ID（1-19） |
| `name` | string | 二选一 | 表情名称 |

**请求示例:**
```bash
# 按 ID
curl -X POST http://127.0.0.1:3210/expression \
  -H 'Content-Type: application/json' \
  -d '{"id": 13}'

# 按名称
curl -X POST http://127.0.0.1:3210/expression \
  -H 'Content-Type: application/json' \
  -d '{"name": "YES"}'
```

---

### POST /motion

播放动作。

**请求体:**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `group` | string | 是 | 动作组名（如 `"Idle"`） |
| `index` | number | 否 | 动作索引（默认 `0`） |

**请求示例:**
```bash
curl -X POST http://127.0.0.1:3210/motion \
  -H 'Content-Type: application/json' \
  -d '{"group": "Idle", "index": 0}'
```

---

### POST /speak

核心接口 — 一次调用完成：显示气泡文字 + 切换表情 + 播放音频 + 口型同步。

**请求体:**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `text` | string | 否 | 气泡显示的文字 |
| `audio_url` | string | 否 | 音频文件路径（支持 `file:///` 绝对路径或 HTTP URL） |
| `audio_data` | string | 否 | base64 编码的音频数据（供远程调用方直接上传音频，无需共享文件系统） |
| `audio_format` | string | 否 | `audio_data` 的格式：`mp3`（默认）/ `wav` / `ogg` / `flac` |
| `expression` | number | 否 | 表情 ID（1-19） |

**行为:**
- 有音频（`audio_url` 或 `audio_data`）时：气泡持续到音频播完自动消失，同时口型随音量同步
- `audio_data` 优先于 `audio_url`：解码后写入 `~/.atri/tmp/speak.<格式>`（每次调用覆盖，即用即弃）
- 无音频时：气泡按文字长度自动计算显示时间（最少 3 秒）

**请求示例:**
```bash
# 完整说话（文字+音频+表情）
curl -X POST http://127.0.0.1:3210/speak \
  -H 'Content-Type: application/json' \
  -d '{
    "text": "ご主人様、こんにちは！",
    "audio_url": "file:///path/to/audio.wav",
    "expression": 1
  }'

# 远程上传音频（base64）— 适用于调用方与桌宠不在同一台机器（如 hermes agent）
curl -X POST http://127.0.0.1:3210/speak \
  -H 'Content-Type: application/json' \
  -d "{
    \"text\": \"高性能ですから！\",
    \"audio_data\": \"$(base64 -i /path/to/audio.mp3)\",
    \"audio_format\": \"mp3\",
    \"expression\": 13
  }"

# 只显示文字+表情（无音频）
curl -X POST http://127.0.0.1:3210/speak \
  -H 'Content-Type: application/json' \
  -d '{"text": "高性能ですから！", "expression": 13}'
```

> 远程集成示例（hermes agent skill）见 [`examples/hermes-skill/`](../examples/hermes-skill/atri-live2d-voice/SKILL.md)。

---

### POST /bubble

仅显示气泡文字（不切换表情、不播放音频）。

**请求体:**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `text` | string | 是 | 显示的文字 |
| `duration` | number | 否 | 显示时长（毫秒，默认 `5000`） |

**请求示例:**
```bash
curl -X POST http://127.0.0.1:3210/bubble \
  -H 'Content-Type: application/json' \
  -d '{"text": "思考中...", "duration": 3000}'
```

---

### POST /lipsync/start

开始播放音频并同步口型。

**请求体:**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `audio_url` | string | 是 | 音频文件路径 |

**请求示例:**
```bash
curl -X POST http://127.0.0.1:3210/lipsync/start \
  -H 'Content-Type: application/json' \
  -d '{"audio_url": "file:///path/to/audio.wav"}'
```

---

### POST /lipsync/stop

停止音频播放和口型同步。

**请求示例:**
```bash
curl -X POST http://127.0.0.1:3210/lipsync/stop
```

---

### GET /audio

本地音频文件代理。将本地文件通过 HTTP 提供访问（内部使用，通常不需要直接调用）。

**Query 参数:**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `path` | string | 是 | 文件绝对路径 |

**请求示例:**
```bash
curl http://127.0.0.1:3210/audio?path=/Users/shine/Downloads/audio.wav --output audio.wav
```

**支持格式:** `.wav`, `.mp3`, `.ogg`, `.flac`

---

## Python 调用示例

```python
import requests

BASE = "http://127.0.0.1:3210"

# 让 ATRI 说话
requests.post(f"{BASE}/speak", json={
    "text": "ご主人様、おはようございます！",
    "audio_url": "file:///path/to/greeting.wav",
    "expression": 1
})

# 切换表情
requests.post(f"{BASE}/expression", json={"name": "YES"})

# 显示气泡
requests.post(f"{BASE}/bubble", json={"text": "正在处理...", "duration": 5000})

# 获取表情列表
expressions = requests.get(f"{BASE}/expressions").json()
```

---

## 音频路径说明

`audio_url` 支持以下格式：

| 格式 | 示例 | 说明 |
|------|------|------|
| `file://` 绝对路径 | `file:///Users/shine/audio.wav` | 自动转为 HTTP 代理访问 |
| 绝对路径 | `/Users/shine/audio.wav` | 同上 |
| HTTP URL | `http://example.com/audio.wav` | 直接使用 |
