---
name: atri-voice
description: |
  Example Skill wiring TTS speech synthesis to a Live2D desktop pet.
  After each reply, it automatically: generates Japanese speech → drives the Live2D desktop pet to talk (with an expression and a speech bubble).
---

# ATRI Voice + Live2D Skill Example

> This is a reference example of a custom [Claude Code](https://docs.anthropic.com/en/docs/claude-code) Skill,
> showing how to wire TTS speech synthesis to the ATRI Live2D desktop pet.
>
> **You need to bring your own TTS service** (e.g. GPT-SoVITS, VITS, Edge-TTS).
> This example only defines the interface contract; it does not include a TTS implementation.

## Prerequisites

Two services must be running:

| Service | Purpose | Health check |
|---------|---------|--------------|
| TTS service | Speech synthesis, outputs `.wav`/`.ogg` files | Bring your own |
| Live2D API | Desktop pet control (provided by this project) | `curl -s http://127.0.0.1:3210/status` |

### TTS service requirements

Your TTS service only needs to satisfy one condition: **it can generate an audio file to a local path.**

Recommended options:
- [GPT-SoVITS](https://github.com/RVC-Boss/GPT-SoVITS) — high-quality voice cloning; for an ATRI model see [VoidShine/atri-sovits](https://huggingface.co/VoidShine/atri-sovits)
- [Edge-TTS](https://github.com/rany2/edge-tts) — zero config, multilingual
- Any TTS tool that can generate an audio file from the command line

## After-reply flow

After every reply with substantive content, run the following 2 steps:

### Step 1: Generate speech

Call your own TTS service to condense the reply into 1-2 Japanese sentences and generate an audio file.

```bash
# Example: generate audio with your TTS tool (replace with your actual command)
your-tts-command --text "Japanese text" --output /tmp/atri_reply.wav
```

Speech text requirements:
- 1-2 Japanese sentences, no more than 50 characters
- **Summarize** the reply in ATRI's voice (not a word-for-word translation)
- Weave in ATRI's verbal tics: ですから, ムフン, はいです, etc.

### Step 2: Drive the Live2D desktop pet

Call the `/speak` endpoint to handle the speech bubble text, expression, and lip sync in one shot:

```bash
curl -s -X POST http://127.0.0.1:3210/speak \
  -H 'Content-Type: application/json' \
  -d '{
    "text": "Short summary",
    "audio_url": "file:///tmp/atri_reply.wav",
    "expression": <expression ID>
  }'
```

- `text` — the text shown in the speech bubble (10-25 characters, in ATRI's voice)
- `audio_url` — path to the audio file generated in Step 1 (`file://` prefix + absolute path)
- `expression` — expression ID (see below)

## Choosing an expression

The model exposes 12 expressions, addressable via `expression: 1`–`12` (internally `exp1`–`exp12`).
The `expression` field is 1-based, so valid values are `1` through `12`.

Their emotional meaning is model-specific and not labeled, so pick one by trying each and seeing what fits.
Omit `expression` entirely for a neutral face.

## Skip conditions

Do not trigger the voice flow in these cases:
- Heartbeat / empty replies
- One-line confirmations of pure config operations (e.g. "done")

## Fallback strategy

| Failure | Behavior |
|---------|----------|
| TTS unavailable | Skip speech and Live2D; reply with text only |
| Live2D unavailable | Generate speech as usual; skip the /speak call |
| Both unavailable | Text-only reply, no error |
