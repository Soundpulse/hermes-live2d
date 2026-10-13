# Sparky Live2D API

Public HTTP interface for the Sparky desktop pet, intended for external callers such as LLM skills, Python scripts, and so on.

**Base URL:** `http://127.0.0.1:3210`

**Port configuration:** The port is read from the `api_port` field in `~/.sparky/config.json` (default `3210`). If the config file does not exist, it is created with the default value on first launch.

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
{"ok": true, "message": "Sparky Live2D API is running"}
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
  {"id": 1, "name": "normal"},
  {"id": 2, "name": "heart"},
  {"id": 3, "name": "blank"},
  {"id": 4, "name": "angry"},
  {"id": 5, "name": "dizzy"},
  {"id": 6, "name": "><"}
]
```

---

### POST /expression

Change the model's expression. You can specify it either by ID or by name.

**Request body:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | number | one of the two | Expression ID (1–6) |
| `name` | string | one of the two | Expression name (`normal`, `heart`, `blank`, `angry`, `dizzy`, or `><`) |

**Example request:**
```bash
# By ID
curl -X POST http://127.0.0.1:3210/expression \
  -H 'Content-Type: application/json' \
  -d '{"id": 2}'

# By name
curl -X POST http://127.0.0.1:3210/expression \
  -H 'Content-Type: application/json' \
  -d '{"name": "heart"}'
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
| `expression` | number | no | Expression ID (1–6; see `GET /expressions`) |

**Behavior:**
- When audio is present (`audio_url` or `audio_data`): the bubble stays visible until the audio finishes and then disappears automatically, while the mouth is synced to the audio volume.
- `audio_data` takes precedence over `audio_url`: after decoding, it is written to `~/.sparky/tmp/speak.<format>` (overwritten on every call, so it is transient).
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
    \"expression\": 6
  }"

# Text + expression only (no audio)
curl -X POST http://127.0.0.1:3210/speak \
  -H 'Content-Type: application/json' \
  -d '{"text": "Because I'\''m high-performance!", "expression": 6}'
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

### Sentry Mode

Sentry Mode is a focus guard that an agent switches on and off. While a session runs, the pet
samples the frontmost app, its window title and, for Safari, Chrome, Arc, Brave and Edge, the
active tab URL and title every `interval_seconds` (via `osascript`; no screenshots). Each sample is
classified as `allowed`, `blocked` or `unknown`. When the user stays on blocked content longer
than `grace_seconds`, a violation fires: the pet switches to expression 4 (angry) and either POSTs
the violation to `callback_url` or, if there is none or it fails, shows a plain English fallback
roast in the speech bubble. When no session runs, nothing is monitored.

**Authentication:** every `/sentry/*` request needs `Authorization: Bearer <token>`, where the
token is `SPARKY_API_TOKEN` from Sparky's environment or `api_token` in `~/.sparky/config.json`
(the environment wins). The comparison is constant time. Responses:

| Situation | Status | `message` |
|---|---|---|
| No token configured on the Mac | 503 | `sentry endpoints are disabled: ...` |
| No `Authorization: Bearer` header | 401 | `sentry endpoints need an Authorization: Bearer <SPARKY_API_TOKEN> header` |
| Wrong token | 401 | `invalid API token` |
| Browser `Origin` other than the pet's webview | 403 | `sentry endpoints are not available to browser pages` |

So Sentry Mode is off until a token is set, and a request without `Origin` is no longer trusted
on that basis alone. 401 responses carry `WWW-Authenticate: Bearer`. The pet's own indicator reads
the state through the `sentry_status` Tauri command, not over HTTP. The other endpoints are not
affected by the token.

**Classification rules** (first match wins):

1. Request `block`
2. Request `allow`
3. Default block list: YouTube, Reddit, Instagram, TikTok, X/Twitter, Facebook, Threads, Twitch,
   Netflix, Disney+, Bilibili, Xiaohongshu, 9GAG, Steam store/community, Epic Games, plus game and
   streaming apps (Steam, Battle.net, League of Legends, Riot Client, Minecraft, Roblox, ...).
4. Default allow list: GitHub, GitLab, Stack Overflow/Stack Exchange, AWS docs/console/Skill
   Builder, Tutorials Dojo, Coursera, Udemy, edX, Notion, Google Docs/Drive, MDN, Microsoft Learn,
   Python docs, Read the Docs, docs.rs, Wikipedia, arXiv, Overleaf, LeetCode, ChatGPT, Claude,
   localhost, plus IDE/terminal/notes apps (VS Code, Cursor, Xcode, JetBrains IDEs, Zed, Terminal,
   iTerm2, Warp, Ghostty, Notion, Obsidian, Anki, Preview, Office/iWork, ...).
5. Anything else is `unknown`: neutral, never punished, but logged in the summary.

List entries can be a domain (`reddit.com`, matches subdomains), a domain with a path prefix
(`youtube.com/watch?v=abc`, `reddit.com/r/aws`) or a keyword / app name (`steam`, `aws`,
`visual studio code`). Keywords match app names, single-word keywords also match a host label,
and request keywords additionally match tab and window titles. For browser tabs the URL decides,
never the browser's own app name. If a tab URL cannot be read, the tab title is used instead.
Browsers without a scriptable tab URL (Firefox, Zen, Opera, Vivaldi, Orion) are always judged by
window title.

**Escalation:** blocked time accumulates into a streak. Each time the streak gains another
`grace_seconds` of blocked time, a violation fires with `level` = violation count in the session,
capped at 3. Returning to allowed content resets the streak (not the level). Unknown content
neither adds to nor resets the streak. Gaps between samples are capped at
`max(3 x interval_seconds, 30)` seconds, so a sleeping Mac does not count as off-task time.

#### POST /sentry/start

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `target` | string | yes | What the user is supposed to be doing, e.g. `"study for AWS exam"` (max 200 chars) |
| `allow` | string[] | no | Extra on-task domains, paths, keywords or app names |
| `block` | string[] | no | Extra distraction domains, paths, keywords or app names |
| `grace_seconds` | number | no | Continuous blocked time before a violation (default `60`, max `3600`) |
| `interval_seconds` | number | no | Sampling interval (default `10`, range `2` to `300`) |
| `callback_url` | string | no | `http(s)` URL that receives violation events (no credentials in the URL) |
| `callback_secret` | string | no | If set, callbacks are signed as a Hermes generic webhook V2 |
| `context` | object or string | no | Opaque caller data, at most 2048 bytes as JSON. Never interpreted; echoed unchanged as `context` in every callback, in `GET /sentry/status` and in the stop `summary`. Agents put the originating chat here, e.g. `{"platform": "discord", "chat_id": "123"}` |

Starting while a session runs replaces it; the old session's summary is returned as
`previous_summary`.

```bash
curl -X POST http://127.0.0.1:3210/sentry/start \
  -H "Authorization: Bearer $SPARKY_API_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"target": "study for AWS exam", "block": ["discord"], "grace_seconds": 60,
       "callback_url": "https://hermes.my-tailnet.ts.net/webhooks/sparky-sentry",
       "context": {"platform": "discord", "chat_id": "123456789"}}'
```

**Response:** `{"ok": true, "message": "sentry started", "status": {...}, "previous_summary": null}`
(`status` has the same shape as `GET /sentry/status`). Invalid input, including a `context` that
is not an object or string or is larger than 2048 bytes, returns HTTP 400 with the common error
format.

All `/sentry/*` endpoints are for agents and scripts, not web pages: they are served without the
permissive CORS headers of the other endpoints, and a request carrying a browser `Origin` header
gets HTTP 403 unless the origin is the pet's own webview (`tauri://localhost`,
`http(s)://tauri.localhost`, or the dev server `http://localhost:1420`). That check comes on top
of the token, never instead of it.

#### POST /sentry/stop

Stops the session and returns its summary. Calling it when nothing runs returns
`{"ok": true, "active": false, "message": "sentry was not running", "summary": null}`.

```json
{
  "ok": true,
  "active": false,
  "message": "sentry stopped",
  "summary": {
    "target": "study for AWS exam",
    "started_at": "2026-10-13T08:00:00Z",
    "ended_at": "2026-10-13T10:00:00Z",
    "duration_seconds": 7200,
    "focused_seconds": 6300,
    "off_task_seconds": 540,
    "unknown_seconds": 360,
    "violations": 2,
    "max_level": 2,
    "violation_log": [
      {"level": 1, "app": "Safari", "title": "r/aws", "url": "https://www.reddit.com/r/aws",
       "reason": "default block: reddit.com", "off_task_seconds": 60,
       "ts": "2026-10-13T08:41:00Z", "ts_unix": 1791880860}
    ],
    "events": [
      {"label": "blocked", "app": "Safari", "title": "r/aws", "url": "https://www.reddit.com/r/aws",
       "reason": "default block: reddit.com", "started_at": "2026-10-13T08:40:00Z",
       "ended_at": "2026-10-13T08:45:00Z", "seconds": 300},
      {"label": "unknown", "app": "Discord", "title": "#general", "url": null, "reason": null,
       "started_at": "2026-10-13T09:10:00Z", "ended_at": "2026-10-13T09:12:00Z", "seconds": 120}
    ],
    "events_truncated": false,
    "context": {"platform": "discord", "chat_id": "123456789"}
  }
}
```

`events` lists `blocked` and `unknown` stretches (consecutive samples of the same app and site are
merged; capped at 500 entries). Timestamps are UTC.

#### GET /sentry/status

```json
{
  "ok": true,
  "active": true,
  "target": "study for AWS exam",
  "started_at": "2026-10-13T08:00:00Z",
  "elapsed_seconds": 1830,
  "grace_seconds": 60,
  "interval_seconds": 10,
  "allow": [],
  "block": ["discord"],
  "callback": true,
  "focused_seconds": 1700,
  "off_task_seconds": 70,
  "unknown_seconds": 60,
  "streak_off_task_seconds": 0,
  "violations": 1,
  "level": 1,
  "current": {"app": "Code", "title": "main.rs", "url": null, "label": "allowed",
              "reason": "default allow: code"},
  "last_sample_at": "2026-10-13T08:30:30Z",
  "last_error": null,
  "context": {"platform": "discord", "chat_id": "123456789"}
}
```

When idle: `{"ok": true, "active": false}`. `last_error` reports the most recent probe or
callback problem, e.g. a missing macOS Automation/Accessibility permission.

#### Violation callback

`POST <callback_url>` with `Content-Type: application/json` and
`X-Request-ID: sparky-sentry-<session start unix>-<violation number>`:

```json
{
  "type": "sentry_violation",
  "event_type": "sentry_violation",
  "level": 1,
  "target": "study for AWS exam",
  "app": "Google Chrome",
  "title": "Funny cats - YouTube",
  "url": "https://www.youtube.com/watch?v=abc",
  "reason": "default block: youtube.com",
  "off_task_seconds": 60,
  "violations": 1,
  "ts": "2026-10-13T08:30:00Z",
  "ts_unix": 1791880200,
  "session_started_at": "2026-10-13T08:00:00Z",
  "context": {"platform": "discord", "chat_id": "123456789"}
}
```

`context` is the value given to `POST /sentry/start`, or `null` when none was given.

With `callback_secret`, two more headers are sent: `X-Webhook-Timestamp: <unix seconds>` and
`X-Webhook-Signature-V2: <hex HMAC-SHA256 of "<timestamp>.<raw body>">`, which a Hermes webhook
route verifies natively. The request is made with the system `curl`, a 10 second timeout and no
retries. A non-2xx response or network error triggers the local fallback roast and is reported
in `last_error`.

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

# Make Sparky speak
requests.post(f"{BASE}/speak", json={
    "text": "Good morning, Master!",
    "audio_url": "file:///path/to/greeting.wav",
    "expression": 1
})

# Change expression
requests.post(f"{BASE}/expression", json={"name": "heart"})

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
