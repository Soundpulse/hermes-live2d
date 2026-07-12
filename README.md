# ATRI Live2D — a desktop pet your agent can speak through

<p align="center">
  <img src="assets/screenshot.png" width="240" alt="ATRI Live2D desktop pet" />
</p>

<p align="center">
  A transparent Live2D desktop pet (Tauri 2 + pixi-live2d-display) with voice lip sync,
  expressions, mouse tracking, and a local HTTP API. Currently ships the <b>Sparkle (火花)</b>
  model. Point your <a href="https://github.com/NousResearch/hermes-agent">hermes agent</a> at it
  and every voice recording it generates is played on the pet — lip-synced, with a speech bubble.
</p>

<p align="center">
  <a href="https://github.com/Soundpulse/hermes-live2d/releases">Releases</a> ·
  <a href="docs/API.md">API reference</a> ·
  <a href="skills/hermes-live2d-voice/SKILL.md">Hermes skill</a> ·
  <a href="examples/claude-code-skill.md">Claude Code skill</a>
</p>

**Useful for:**

- Health checks
- Cron reminders
- Looking cute

---

## Enable this for your hermes agent

This is the main use case. Your hermes agent runs somewhere (e.g. an EC2 box, chatting over
the Discord gateway); the pet runs on your Mac. Whenever the agent generates a TTS recording
with its `text_to_speech` tool, a small skill uploads that audio to the pet, which plays it with
lip sync + expression + a speech bubble. Normal chat replies are **not** spoken — only recordings
the agent was already producing get routed.

```
hermes agent (remote)                         your Mac
────────────────────                          ────────
text_to_speech ──► hermes-live2d-voice skill ──► POST /speak ──► Sparkle pet
                     (speak.sh, base64)          (over Tailscale)   lip sync + bubble
```

### Requirements

- **[Tailscale](https://tailscale.com)** on both the Mac and the agent host (same tailnet) — this
  is how the remote agent reaches the pet without exposing it publicly.
- **A hermes agent wired to a TTS engine** — any engine works (ElevenLabs, etc.), as long as the
  agent produces audio files via its `text_to_speech` tool. That's the audio the pet plays.
- **The `hermes-live2d-voice` skill** — bundled in this repo at
  [`skills/hermes-live2d-voice/`](skills/hermes-live2d-voice/)
  (`SKILL.md` + `speak.sh`); you install it on the agent host in step 3.

### 1. Install the desktop pet (Mac)

Requirements: **macOS on Apple Silicon** (the app uses macOS-private window APIs and builds only
for `aarch64-apple-darwin`).

Grab a build from [Releases](https://github.com/Soundpulse/hermes-live2d/releases), or build from
source:

```bash
pnpm install
pnpm tauri build          # release .app / .dmg under src-tauri/target/release/bundle/
# or, during development:
pnpm tauri dev
```

**Install the Live2D model.** The model assets are **not** included in this repo (they're
gitignored) and are not redistributed here — download them and drop them in yourself:

1. Download the free **Spark (火花)** model from
   [booth.pm/en/items/8265367](https://booth.pm/en/items/8265367) and unzip it.
2. Place the unzipped model folder so its entry file is named exactly **`Sparkle.model3.json`**
   (rename the `.model3.json` if the download uses a different name), in one of:
   - **`~/.atri/model/`** — runtime override, no rebuild needed. Loaded on the next launch and
     takes priority. Result: `~/.atri/model/Sparkle.model3.json` (plus its `.moc3`, textures,
     physics, expressions, etc. alongside).
   - **`public/model/`** — bundled into the app at build time (use this when building from source;
     required, since the repo ships no model). Result: `public/model/Sparkle.model3.json`.

To use a different Live2D character entirely, put its files in `~/.atri/model/` with the entry
file renamed to `Sparkle.model3.json` — the filename is what the app looks for. Note its
expressions/keyforms will differ (see the skill's model-swap note).

Launch the app. On first run it creates `~/.atri/config.json` and starts a local HTTP API on
`http://127.0.0.1:3210`. Confirm it's up:

```bash
curl http://127.0.0.1:3210/status
# {"ok":true,"message":"ATRI Live2D API is running"}
```

### 2. Expose the pet to your agent over Tailscale

The API binds to loopback only (`127.0.0.1`) and has no auth, so it must **not** be exposed
directly. Put it on your tailnet instead. On the Mac:

```bash
tailscale serve --bg 3210
```

This publishes the pet at `https://<mac-name>.<tailnet>.ts.net`, reachable only by your own
devices.

### 3. Install the hermes skill (agent host)

Copy the bundled skill into your hermes agent's skills directory:

```bash
cp -r skills/hermes-live2d-voice /path/to/hermes/skills/
```

It's a standard `SKILL.md` + `speak.sh` bundle. Tell the agent's environment where the pet lives:

```bash
export ATRI_SPEAK_URL="https://<mac-name>.<tailnet>.ts.net"
```

Verify from the agent host:

```bash
curl -s "$ATRI_SPEAK_URL/status"
# {"ok":true,"message":"ATRI Live2D API is running"}
```

That's it. See [`skills/hermes-live2d-voice/SKILL.md`](skills/hermes-live2d-voice/SKILL.md)
for the exact contract, the expression list, and the graceful-degrade behavior when the Mac is
asleep or offline.

---

## Features

- **Transparent desktop pet** — frameless, transparent, always-on-top; draggable and resizable
- **Mouse tracking** — eyes and head follow the cursor in real time, across multiple monitors
- **Voice lip sync** — mouth animation is driven by audio volume during playback
- **Expressions & motions** — 12 expressions (`exp1`–`exp12`) plus custom motion playback
- **Speech bubbles** — typewriter-style bubbles with configurable duration, anchored to the head
- **HTTP API** — local REST API (default port 3210) for external programs to drive the pet
- **Window memory** — remembers window position and size across launches
- **System tray** — tray icon to lock/unlock and quit

## HTTP API at a glance

Once the app is running, the API listens on `http://127.0.0.1:3210`.

```bash
# Health check
curl http://127.0.0.1:3210/status

# Make the pet speak (text + audio + expression)
curl -X POST http://127.0.0.1:3210/speak \
  -H 'Content-Type: application/json' \
  -d '{"text": "hello!", "audio_url": "file:///path/to/audio.wav", "expression": 1}'

# Switch expression (id 1–12, or name exp1–exp12)
curl -X POST http://127.0.0.1:3210/expression \
  -H 'Content-Type: application/json' \
  -d '{"name": "exp1"}'

# Show a speech bubble
curl -X POST http://127.0.0.1:3210/bubble \
  -H 'Content-Type: application/json' \
  -d '{"text": "thinking...", "duration": 3000}'
```

`/speak` also accepts inline audio as base64 (`audio_data` + `audio_format`) instead of an
`audio_url` — that's how the remote hermes skill sends recordings across the network. Full
endpoint reference: [docs/API.md](docs/API.md).

### Expressions

The model exposes **12 expressions**, addressed as `expression: 1`–`12` (names `exp1`–`exp12`, ids
are 1-based); omit `expression` for the neutral/default face. Each id maps to a specific mood on
the loaded model — see the mood table in
[`skills/hermes-live2d-voice/SKILL.md`](skills/hermes-live2d-voice/SKILL.md#expression-guide) for
the current mapping. Query the live id/name list any time with
`curl http://127.0.0.1:3210/expressions`.

## Configuration

Config lives at `~/.atri/config.json`, created on first launch:

```json
{
  "api_port": 3210
}
```

- Window position/size are saved to `~/.atri/window_state.json`.
- Uploaded `/speak` audio is written to `~/.atri/tmp/` and overwritten on each call.
- Drop a custom Live2D model under `~/.atri/model/` to load it instead of the bundled Sparkle
  model on startup.

The bundled character is the free **Spark (火花)** Live2D model by 夜墨ww
([booth.pm/en/items/8265367](https://booth.pm/en/items/8265367)) — Sparkle from Honkai: Star Rail.

## Other integrations

- **Claude Code** — [`examples/claude-code-skill.md`](examples/claude-code-skill.md) is a
  TTS-agnostic example skill: generate speech with your own TTS (GPT-SoVITS, Edge-TTS, …) and
  drive the pet after each reply.
- **GPT-SoVITS voice (optional)** — for a Japanese ATRI voice, a GPT-SoVITS model is hosted at
  [VoidShine/atri-sovits](https://huggingface.co/VoidShine/atri-sovits). Not needed for the hermes
  path, where the agent supplies its own TTS audio.

## Tech stack

| Layer | Technology |
|-------|-----------|
| Desktop framework | [Tauri 2](https://v2.tauri.app) (Rust) |
| Rendering | [PixiJS 6](https://pixijs.com) + [pixi-live2d-display](https://github.com/guansss/pixi-live2d-display) |
| Live2D | Cubism 4 SDK |
| API server | [Axum](https://github.com/tokio-rs/axum) |
| TTS (optional) | [GPT-SoVITS](https://github.com/RVC-Boss/GPT-SoVITS) |

## Limitations

- **macOS only** — relies on macOS-private APIs (transparent window, click-through, etc.); no
  Windows or Linux support.
- **Apple Silicon only** — release target is `aarch64-apple-darwin`.
- **Model-bound parameters** — expression/keyform mappings are hardcoded for the bundled model;
  swapping in a different Live2D model requires code changes.
- **Volume-driven lip sync** — mouth movement maps from audio volume, not viseme-accurate phonemes.
- **Local, unauthenticated API** — the HTTP API listens on `127.0.0.1` with no auth; only expose
  it to trusted devices (e.g. a private tailnet), never the public internet.
- **Mouse tracking via Tauri** — `cursor_position()` behavior can vary across macOS versions.

## License

Project code is released under the MIT license.

Use of the Live2D Cubism SDK and the character model assets is subject to their respective
licenses. The bundled **Spark (火花)** model is by 夜墨ww
([booth.pm/en/items/8265367](https://booth.pm/en/items/8265367)); follow its terms.
