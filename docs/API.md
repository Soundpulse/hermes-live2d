# ATRI Live2D API

Public HTTP interface for the ATRI desktop pet, intended for external callers such as LLM skills, Python scripts, and so on.

**Base URL:** `http://127.0.0.1:3210`

**Port configuration:** The port is read from the `api_port` field in `~/.atri/config.json` (default `3210`). If the config file does not exist, it is created with the default value on first launch.

---

## Common Response Format

```json
{
  "ok": true,
  "message": "description"
}
```

On error, the API returns HTTP 400:
```json
{
  "ok": false,
  "message": "reason for failure"
}
```

---

## Endpoints

### GET /status

Check whether the API service is running.

**Example request:**
```bash
curl http://127.0.0.1:3210/status
```

**Response:**
```json
{"ok": true, "message": "ATRI Live2D API is running"}
```

---

### GET /expressions

Get the list of all available expressions.

**Example request:**
```bash
curl http://127.0.0.1:3210/expressions
```

**Response:**
```json
[
  {"id": 1, "name": "exp1"},
  {"id": 2, "name": "exp2"},
  {"id": 3, "name": "exp3"},
  {"id": 4, "name": "exp4"},
  {"id": 5, "name": "exp5"},
  {"id": 6, "name": "exp6"},
  {"id": 7, "name": "exp7"},
  {"id": 8, "name": "exp8"},
  {"id": 9, "name": "exp9"},
  {"id": 10, "name": "exp10"},
  {"id": 11, "name": "exp11"},
  {"id": 12, "name": "exp12"}
]
```

---

### POST /expression

Change the model's expression. You can specify it either by ID or by name.

**Request body:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | number | one of the two | Expression ID (1–12) |
| `name` | string | one of the two | Expression name (`exp1`–`exp12`) |

**Example request:**
```bash
# By ID
curl -X POST http://127.0.0.1:3210/expression \
  -H 'Content-Type: application/json' \
  -d '{"id": 5}'

# By name
curl -X POST http://127.0.0.1:3210/expression \
  -H 'Content-Type: application/json' \
  -d '{"name": "exp5"}'
```

---

### POST /motion

Play a motion.

**Request body:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `group` | string | yes | Motion group name (e.g. `"Idle"`) |
| `index` | number | no | Motion index (default `0`) |

**Example request:**
```bash
curl -X POST http://127.0.0.1:3210/motion \
  -H 'Content-Type: application/json' \
  -d '{"group": "Idle", "index": 0}'
```

---

### POST /speak

The core endpoint — a single call that shows the speech bubble text, changes the expression, plays the audio, and drives lip sync all at once.

**Request body:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `text` | string | no | Text to display in the speech bubble |
| `audio_url` | string | no | Audio file path (supports a `file:///` absolute path or an HTTP URL) |
| `audio_data` | string | no | Base64-encoded audio data (lets a remote caller upload audio directly, with no shared filesystem required) |
| `audio_format` | string | no | Format of `audio_data`: `mp3` (default) / `wav` / `ogg` / `flac` |
| `expression` | number | no | Expression ID (1–12; id 1 = `exp1` … id 12 = `exp12`) |

**Behavior:**
- When audio is present (`audio_url` or `audio_data`): the bubble stays visible until the audio finishes and then disappears automatically, while the mouth is synced to the audio volume.
- `audio_data` takes precedence over `audio_url`: after decoding, it is written to `~/.atri/tmp/speak.<format>` (overwritten on every call, so it is transient).
- When no audio is present: the bubble's display time is computed automatically from the text length (minimum 3 seconds).

**Example requests:**
```bash
# Full utterance (text + audio + expression)
curl -X POST http://127.0.0.1:3210/speak \
  -H 'Content-Type: application/json' \
  -d '{
    "text": "Hello, Master!",
    "audio_url": "file:///path/to/audio.wav",
    "expression": 1
  }'

# Remote audio upload (base64) — for when the caller and the pet are not on the
# same machine (e.g. the hermes agent)
curl -X POST http://127.0.0.1:3210/speak \
  -H 'Content-Type: application/json' \
  -d "{
    \"text\": \"Because I'm high-performance!\",
    \"audio_data\": \"$(base64 -i /path/to/audio.mp3)\",
    \"audio_format\": \"mp3\",
    \"expression\": 12
  }"

# Text + expression only (no audio)
curl -X POST http://127.0.0.1:3210/speak \
  -H 'Content-Type: application/json' \
  -d '{"text": "Because I'\''m high-performance!", "expression": 12}'
```

> See [`skills/hermes-live2d-voice/`](../skills/hermes-live2d-voice/SKILL.md) for a remote integration example (a hermes agent skill).

---

### POST /bubble

Show only the speech bubble text (no expression change, no audio playback).

**Request body:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `text` | string | yes | Text to display |
| `duration` | number | no | Display duration (milliseconds, default `5000`) |

**Example request:**
```bash
curl -X POST http://127.0.0.1:3210/bubble \
  -H 'Content-Type: application/json' \
  -d '{"text": "Thinking...", "duration": 3000}'
```

---

### POST /lipsync/start

Start playing audio and sync the mouth to it.

**Request body:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `audio_url` | string | yes | Audio file path |

**Example request:**
```bash
curl -X POST http://127.0.0.1:3210/lipsync/start \
  -H 'Content-Type: application/json' \
  -d '{"audio_url": "file:///path/to/audio.wav"}'
```

---

### POST /lipsync/stop

Stop audio playback and lip sync.

**Example request:**
```bash
curl -X POST http://127.0.0.1:3210/lipsync/stop
```

---

### GET /audio

Local audio file proxy. Serves a local file over HTTP (used internally; you normally do not need to call it directly).

**Query parameters:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | yes | Absolute path to the file |

**Example request:**
```bash
curl http://127.0.0.1:3210/audio?path=/Users/shine/Downloads/audio.wav --output audio.wav
```

**Supported formats:** `.wav`, `.mp3`, `.ogg`, `.flac`

---

## Python Example

```python
import requests

BASE = "http://127.0.0.1:3210"

# Make ATRI speak
requests.post(f"{BASE}/speak", json={
    "text": "Good morning, Master!",
    "audio_url": "file:///path/to/greeting.wav",
    "expression": 1
})

# Change expression
requests.post(f"{BASE}/expression", json={"name": "exp5"})

# Show a bubble
requests.post(f"{BASE}/bubble", json={"text": "Working on it...", "duration": 5000})

# Get the expression list
expressions = requests.get(f"{BASE}/expressions").json()
```

---

## Audio Path Notes

`audio_url` supports the following formats:

| Format | Example | Notes |
|--------|---------|-------|
| `file://` absolute path | `file:///Users/shine/audio.wav` | Automatically routed through the HTTP proxy |
| Absolute path | `/Users/shine/audio.wav` | Same as above |
| HTTP URL | `http://example.com/audio.wav` | Used directly |
